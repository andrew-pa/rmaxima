extern crate runic;
extern crate winit;
extern crate mio;
extern crate regex;
extern crate xml;

use runic::*;
use winit::*;

use std::error::Error;
use std::process::*;
use std::io::{Read,Write};
//use mio::net::TcpStream;
use std::net::TcpStream;
use std::fmt::Display;

use regex::Regex;

mod mathml;

struct Cell {
    index: usize,
    input: String,
    output: Option<mathml::MathExpression>,
    output_src: Option<String>,
    input_layout: Option<TextLayout>
}

impl Cell {
    fn empty(index: usize) -> Cell {
        Cell {
            index, input: String::new(),
            output: None, output_src: None, input_layout: None
        }
    }

    fn bounds(&self) -> Rect {
       let ib = self.input_layout.as_ref().map(|ly| ly.bounds()).unwrap_or(Rect::wh(0.0, 0.0));
       let ob = self.output.as_ref().map(|e| e.bounds()).unwrap_or(Rect::wh(0.0, 0.0));
       Rect::wh(ib.w.max(ob.w), ib.h+ob.h+4.0)
    }

    fn draw(&mut self, p: Point, rx: &mut RenderContext, fnt: &Font, math_fnt: &Font) {
        let input_str = format!("(%{}) {}", self.index, self.input);
        let ily = self.input_layout.get_or_insert_with(|| {
            rx.new_text_layout(&input_str, &fnt, 4096.0, 256.0).expect("create text layout")
        });
        rx.draw_text_layout(p, ily);
        let ib = ily.bounds();
        if self.output_src.is_some() {
            self.output = match mathml::MathExpression::from_mathml(self.output_src.take().unwrap().as_bytes(), rx, &math_fnt) {
                Ok(o) => Some(o),
                Err(e) => {
                    println!("mathml error: {}", e);
                    None
                }
            };
        }
        if let Some(ref o) = self.output {
            let ob = o.bounds();
            o.draw(p + Point::y(ib.h+4.0 + ob.h/2.0), rx);
        }
    }

    fn draw_cursor(&self, p: Point, rx: &mut RenderContext, cursor_idx: usize) {
        let cb = self.input_layout.as_ref().map(|ly| ly.char_bounds(cursor_idx+4+self.index/10)).unwrap().offset(p);
        rx.set_color(Color::rgba(0.6, 0.6, 0.8, 0.9));
        rx.draw_line(Point::xy(cb.x+cb.w, cb.y), Point::xy(cb.x+cb.w, cb.y+cb.h), 2.0);
        rx.set_color(Color::rgb(0.8, 0.75, 0.7));
    }
}

struct MaximaApp {
    font: Font, math_font: Font,
    maxima_proc: Child,
    maxima_strm: TcpStream,
    cells: Vec<Cell>,
    current_cell: usize,
    cursor_idx: usize,
    input_regex: Regex,
    output_regex: Regex,
    viewport_start: usize,
}

impl MaximaApp {
    fn new(rx: &mut RenderContext) -> Result<MaximaApp, Box<Error>> {
        let font = rx.new_font("Fira Code", 18.0, FontWeight::Regular, FontStyle::Normal)?;
        let math_font = rx.new_font("Cambria Math", 18.0, FontWeight::Regular, FontStyle::Normal)?;
        let mut proc = Command::new("C:/maxima-5.41.0a/clisp-2.49/base/lisp.exe")
            .args(vec!["-q", "-M", "C:/maxima-5.41.0a/lib/maxima/5.41.0a_dirty/binary-clisp/maxima.mem",
                  "", "--", "-r", ":lisp ($load \"mathml\") (defun displa(exp) (print (cadr exp)) (mathml1 (caddr exp)) (terpri)) (setup-client 4444)\n"])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
        let listener = std::net::TcpListener::bind("127.0.0.1:4444").unwrap();
        let mut strm = listener.accept()?.0;
        strm.set_nonblocking(true)?;
        Ok(MaximaApp {
            font, math_font,
            maxima_proc: proc,
            maxima_strm: strm,
            cells: Vec::new(), current_cell: 0, cursor_idx: 0,
            input_regex: Regex::new(r"\(%i(\d+)\)")?,
            output_regex: Regex::new(r"(?ms)\$%O(\d+)\s[[:cntrl:]]*(.*</math>)")?,
            viewport_start: 0
        })
    }
}

impl MaximaApp {
    fn update(&mut self, rx: &mut RenderContext) {
        let mut new_in = String::new();
        let mut buf = [0; 512];
        loop {
            match self.maxima_strm.read(&mut buf) {
                Ok(len) => {
                    if len == 0 { break; }
                    new_in += &String::from_utf8_lossy(&buf[0..len]);
                },
                Err(e) => {
                    match e.kind() {
                        std::io::ErrorKind::WouldBlock => break,
                        _ => panic!("error reading from stream: {:?}", e)
                    }
                }
            }
        }
        if new_in.len() > 0 {
            println!("in: \"{}\"", new_in);
            for outputs in self.output_regex.captures_iter(&new_in) {
                let index = outputs[1].parse().expect("parse output index");
                let mut found = false;
                for cell in self.cells.iter_mut() {
                    if cell.index == index {
                        cell.output_src = Some(String::from(outputs[2].trim()));
                        cell.output = None;
                        found = true;
                        break;
                    }
                }
                if !found {
                    let mut c = Cell::empty(index);
                    c.output_src = Some(String::from(outputs[2].trim()));
                    self.cells.push(c);
                }
            }
            if let Some(ref inp) = self.input_regex.captures(&new_in) {
                let index = inp[1].parse().expect("parse output index");
                self.cells.push(Cell::empty(index));
                self.current_cell = self.cells.len()-1;
                self.cursor_idx = 0;
            }
        }
    }
}

impl App for MaximaApp {

    fn paint(&mut self, rx: &mut RenderContext) {
        let bnds = rx.bounds();
        self.update(rx);
        rx.clear(Color::rgb(0.0, 0.0, 0.0));
        rx.set_color(Color::rgb(0.8, 0.75, 0.7));
        let mut p = Point::xy(8.0, 8.0);
        let fnt = self.font.clone();
        let math_fnt = self.math_font.clone();
        for (i, c) in self.cells.iter_mut().enumerate().skip(self.viewport_start) {
            c.draw(p, rx, &fnt, &math_fnt);
            let b = c.bounds();
            if i == self.current_cell {
                c.draw_cursor(p, rx, self.cursor_idx);
            }
            p.y += b.h + 4.0;
            if p.y > bnds.h {
                self.viewport_start += 1;
                break;
            }
        }
    }

    fn event(&mut self, e: Event) -> bool {
        let cell = self.current_cell;
        match e {
            Event::WindowEvent { event: WindowEvent::ReceivedCharacter(c), .. } => {
                if !c.is_control() { 
                    let cc = self.cursor_idx;
                    self.cells[cell].input.insert(cc, c);
                    self.cells[cell].input_layout = None;
                    self.cursor_idx += 1;
                }
            },
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput {
                    input: KeyboardInput {
                        virtual_keycode: Some(k),
                        modifiers: mods,
                        state: ElementState::Pressed, ..
                    }
                           , ..
                }, .. } => {
                    match k {
                        VirtualKeyCode::Return => {
                            if mods.shift {
                                self.cells[cell].input.push(';');
                                self.cells[cell].input_layout = None;
                            }
                            write!(self.maxima_strm, "{}", self.cells[cell].input).expect("write stream");
                        }
                        VirtualKeyCode::Left => { if self.cursor_idx > 0 { self.cursor_idx -= 1; } }
                        VirtualKeyCode::Right => {
                            let len = self.cells[cell].input.len();
                            if self.cursor_idx < len { self.cursor_idx += 1; }
                        }
                        VirtualKeyCode::Back => {
                            if self.cursor_idx > 0 && self.cells[cell].input.len() != 0 {
                                self.cursor_idx -= 1;
                                self.cells[cell].input.remove(self.cursor_idx);
                                self.cells[cell].input_layout = None;
                            }
                        }
                        VirtualKeyCode::PageUp => { if self.viewport_start > 0 { self.viewport_start -= 1; } }
                        VirtualKeyCode::PageDown => { if self.viewport_start < self.cells.len() { self.viewport_start += 1; } }
                        /*VirtualKeyCode::Up => { if self.current_cell > 0 { self.current_cell -= 1; } }
                          VirtualKeyCode::Down => { if self.current_cell < self.cells.len() { self.current_cell += 1; } }*/
                        _ => {}
                    }
                }
            _ => {}
        }
        false
    }
}

impl Drop for MaximaApp {
    fn drop(&mut self) {
        self.maxima_proc.kill().expect("end maxima client!");
    }
}

fn main() -> Result<(), Box<Error>> {
    runic::init();
    let mut evl = EventsLoop::new();
    let mut window = WindowBuilder::new().with_dimensions(640, 400).with_title("rMaxima").build(&evl)?;
    let mut rx = RenderContext::new(&mut window)?;
    let mut app = MaximaApp::new(&mut rx)?;
    Ok(app.run(&mut rx, &mut evl))
}
