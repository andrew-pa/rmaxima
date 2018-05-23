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
    output: Option<String>,
}

impl Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "(%i{}) {}", self.index, self.input)?;
        if let Some(ref s) = self.output {
            write!(f, "\n(%o{}) {}", self.index, s)?;
        }
        Ok(())
    }
}

struct MaximaApp {
    font: Font,
    maxima_proc: Child,
    maxima_strm: TcpStream,
    cells: Vec<(Cell, Option<TextLayout>)>,
    current_cell: usize,
    cursor_idx: usize,
    input_regex: Regex,
    output_regex: Regex,
    mx: mathml::MathExpression
}

impl MaximaApp {
    fn new(rx: &mut RenderContext) -> Result<MaximaApp, Box<Error>> {
        let font = rx.new_font("Fira Code", 18.0, FontWeight::Regular, FontStyle::Normal)?;
        let mut proc = Command::new("C:/maxima-5.41.0a/clisp-2.49/base/lisp.exe")
            .args(vec!["-q", "-M", "C:/maxima-5.41.0a/lib/maxima/5.41.0a_dirty/binary-clisp/maxima.mem", "", "--", "-r", ":lisp (setup-client 4444)"])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
        let listener = std::net::TcpListener::bind("127.0.0.1:4444").unwrap();
        let mut strm = listener.accept()?.0;
        strm.set_nonblocking(true)?;
        write!(strm, "display2d:false;");
        let mx = mathml::MathExpression::from_mathml("<math><mi>q</mi><mo>*</mo><mroot><mfrac><mrow><mn>3</mn><mo>+</mo><mi>a</mi></mrow><mn>7</mn></mfrac><mrow><mn>3</mn><mo>-</mo><mi>d</mi></mrow></mroot></math>".as_bytes(), rx, &font)?;
        Ok(MaximaApp {
            font,
            maxima_proc: proc,
            maxima_strm: strm,
            cells: Vec::new(), current_cell: 0, cursor_idx: 0,
            input_regex: Regex::new(r"\(%i(\d+)\)")?,
            output_regex: Regex::new(r"\(%o(\d+)\)(.+)")?,
            mx
        })
    }
}

impl App for MaximaApp {

    fn paint(&mut self, rx: &mut RenderContext) {
        let bnds = rx.bounds();
        let mut data = [0; 512];
        match self.maxima_strm.read(&mut data) {
            Ok(len) => {
                let s = String::from_utf8_lossy(&data[0..len]);
                for ln in s.lines() {
                    if let Some(ref r) = self.input_regex.captures(&ln) {
                        println!("i cap = {:?}", r);
                        let index = r[1].parse().expect("parse index");
                        if let Some((i, _)) = self.cells.iter().enumerate().find(|(_,c)| c.0.index == index) {
                            self.current_cell = i; // this is probably an error state. should parse out the error too
                            self.cells[i].1 = None;
                        } else {
                            self.cells.push((Cell { index: index, input: String::new(), output: None }, None));
                            self.current_cell = self.cells.len()-1;
                            self.cursor_idx = 0;
                        }
                    } else if let Some(ref r) = self.output_regex.captures(&ln) {
                        println!("o cap = {:?}", r);
                        let index = r[1].parse().expect("parse index");
                        let found = self.cells.iter_mut().find(|c| c.0.index == index).map(|cell| {
                            cell.0.output = Some(String::from(r[2].trim()));
                            cell.1 = None;
                        }).is_some();
                        if !found {
                            self.cells.push((Cell { index: index, input: String::new(), output: Some(String::from(r[2].trim())) }, None));
                        }
                    } else {
                        println!("unk = {}", ln);
                    }
                    //self.lines.push((String::from(ln), rx.new_text_layout(&ln, &self.font, bnds.w, 256.0).expect("create text layout")));
                }
            },
            Err(e) => {
                if let std::io::ErrorKind::WouldBlock = e.kind()  {

                } else {
                    panic!("error reading from stream: {:?}", e);
                }
            } 
        }
        rx.clear(Color::rgb(0.0, 0.0, 0.0));
        rx.set_color(Color::rgb(0.8, 0.75, 0.7));
        let mut p = Point::xy(8.0, 8.0);
        let fnt = self.font.clone();
        for (i, (c, ly)) in self.cells.iter_mut().enumerate() {
            let ly = ly.get_or_insert_with(|| rx.new_text_layout(&format!("{}",c), &fnt, bnds.w, 256.0).expect("create text layout"));
            rx.draw_text_layout(p, ly);
            let b = ly.bounds();
            if i == self.current_cell {
                /*rx.set_color(Color::rgb(0.8, 0.35, 0.0));
                rx.draw_line(Point::xy(p.x-2.0, p.y), Point::xy(p.x-2.0, p.y+b.h), 1.0);*/
                let cb = ly.char_bounds(self.cursor_idx+5).offset(p);
                rx.set_color(Color::rgba(0.6, 0.6, 0.8, 0.9));
                rx.draw_line(Point::xy(cb.x+cb.w, cb.y), Point::xy(cb.x+cb.w, cb.y+cb.h), 2.0);
                rx.set_color(Color::rgb(0.8, 0.75, 0.7));
            }
            p.y += b.h + 4.0;
        }
        self.mx.draw(Point::xy(8.0, 100.0), rx);
    }

    fn event(&mut self, e: Event) -> bool {
        let cell = self.current_cell;
        match e {
            Event::WindowEvent { event: WindowEvent::ReceivedCharacter(c), .. } => {
                if !c.is_control() { 
                    let cc = self.cursor_idx;
                    self.cells[cell].0.input.insert(cc, c);
                    self.cells[cell].1 = None;
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
                                self.cells[cell].0.input.push(';');
                            }
                            write!(self.maxima_strm, "{}", self.cells[cell].0.input).expect("write stream");
                        }
                        VirtualKeyCode::Left => { if self.cursor_idx > 0 { self.cursor_idx -= 1; } }
                        VirtualKeyCode::Right => {
                            let len = self.cells[cell].0.input.len();
                            if self.cursor_idx < len { self.cursor_idx += 1; }
                        }
                        VirtualKeyCode::Back => {
                            if self.cursor_idx > 0 && self.cells[cell].0.input.len() != 0 {
                                self.cursor_idx -= 1;
                                self.cells[cell].0.input.remove(self.cursor_idx);
                                self.cells[cell].1 = None;
                            }
                        }
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
