
use runic::*;

use xml::name::OwnedName;
use xml::reader::{EventReader, XmlEvent, Error as XmlError};

use std::io::Read;
use std::error::Error;

#[derive(Debug)]
pub enum MathMLParseError {
    XMLError(XmlError),
    UnexpectedXMLEvent(XmlEvent),
    UnexpectedXMLTag(String),
    AppendToLeaf,
    AppendToFullNode
}

impl ::std::fmt::Display for MathMLParseError {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match self {
            &MathMLParseError::AppendToLeaf => write!(fmt, "attempt to append to leaf node"),
            &MathMLParseError::AppendToFullNode => write!(fmt, "attempt to append to full node"),
            &MathMLParseError::XMLError(ref e) => write!(fmt, "xml error: {}", e),
            &MathMLParseError::UnexpectedXMLEvent(ref e) => write!(fmt, "unexpected XML event: {:?}", e),
            &MathMLParseError::UnexpectedXMLTag(ref s) => write!(fmt, "unexpected XML tag: {}", s)
        }
    }
}

impl Error for MathMLParseError {
    fn description(&self) -> &str {
        match self {
            &MathMLParseError::XMLError(ref e) => e.description(),
            &MathMLParseError::UnexpectedXMLEvent(_) => "unexpected XML event",
            &MathMLParseError::UnexpectedXMLTag(_) => "unexpected XML tag",
            &MathMLParseError::AppendToLeaf => "attempt to append to leaf",
            &MathMLParseError::AppendToFullNode => "attempt to append to full node"
        }
    }
}

impl From<XmlError> for MathMLParseError {
    fn from(e: XmlError) -> MathMLParseError {
        MathMLParseError::XMLError(e)
    }
}

#[derive(Clone)]
pub enum Element {
    ParsePlaceholder,
    Id(String, Option<TextLayout>),
    Number(String, Option<TextLayout>),
    Operator(String, Option<TextLayout>),
    Space(usize),
    Row(Vec<Element>),
    Fraction { numer: Box<Element>, denom: Box<Element> },
    Sqrt(Box<Element>),
    Root { base: Box<Element>, index: Box<Element> },
    Subscript { base: Box<Element>, script: Box<Element> },
    Superscript { base: Box<Element>, script: Box<Element> },
    Subsuperscript { base: Box<Element>, subscript: Box<Element>, superscript: Box<Element> }
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    Rect::xywh(a.x.min(b.x), a.y.min(b.y), a.w.max(b.w), a.h.max(b.h))
}

impl Element {
    fn is_placeholder(&self) -> bool {
        match self {
            &Element::ParsePlaceholder => true,
            _ => false
        }
    }
    fn is_row(&self) -> bool {
        match self {
            &Element::Row(_) => true,
            _ => false
        }
    }

    fn set_body(&mut self, s: String, rx: &mut RenderContext, fnt: &Font) -> Result<(), MathMLParseError> {
        match self {
            &mut Element::Id(ref mut body, ref mut layout) |
            &mut Element::Number(ref mut body, ref mut layout) |
            &mut Element::Operator(ref mut body, ref mut layout) => {
                *layout = Some(rx.new_text_layout(&s, fnt, 512.0, 512.0).expect("create text layout"));
                *body = s;
            }
            _ => return Err(MathMLParseError::AppendToFullNode)
        }
        Ok(())
    }

    fn append_child(&mut self, e: Element) -> Result<(), MathMLParseError> {
        match self {
            &mut Element::Row(ref mut children) => { children.push(e); Ok(()) },
            &mut Element::Fraction { ref mut numer, ref mut denom } => {
                if numer.is_placeholder() {
                    *numer = Box::new(e);
                } else if denom.is_placeholder() {
                    *denom = Box::new(e);
                } else {
                    return Err(MathMLParseError::AppendToFullNode)
                }
                Ok(())
            },
            &mut Element::Sqrt(ref mut child) => {
                if child.is_placeholder() {
                    *child = Box::new(e);
                } else if child.is_row() {
                    child.append_child(e)?;
                } else {
                    let new_child = Box::new(Element::Row(vec![*child.clone(), e]));
                    *child = new_child;
                }
                Ok(())
            },
            &mut Element::Root { ref mut base, ref mut index } => {
                if base.is_placeholder() {
                    *base = Box::new(e);
                } else if index.is_placeholder() {
                    *index = Box::new(e);
                } else {
                    return Err(MathMLParseError::AppendToFullNode)
                }
                Ok(())
            },
            &mut Element::Subscript { ref mut base, ref mut script } | Element::Superscript { ref mut base, ref mut script } => {
                if base.is_placeholder() {
                    *base = Box::new(e);
                } else if script.is_placeholder() {
                    *script = Box::new(e);
                } else {
                    return Err(MathMLParseError::AppendToFullNode)
                }
                Ok(())
            },
            &mut Element::Subsuperscript { ref mut base, ref mut subscript, ref mut superscript } => {
                if base.is_placeholder() {
                    *base = Box::new(e);
                } else if subscript.is_placeholder() {
                    *subscript = Box::new(e);
                } else if superscript.is_placeholder() {
                    *superscript = Box::new(e);
                } else {
                    return Err(MathMLParseError::AppendToFullNode)
                }
                Ok(())
            },
            _ => Err(MathMLParseError::AppendToLeaf)
        }
    }

    fn from_mathml<R: Read>(reader: &mut EventReader<R>, rx: &mut RenderContext, fnt: &Font) -> Result<Element, MathMLParseError> {
        let mut els: Vec<Element> = Vec::new();

        loop {
            match reader.next()? {
                XmlEvent::StartElement { name, .. } => {
                    els.push(match name.local_name.as_str()  {
                        "mi" => Element::Id(String::new(), None),
                        "mo" => Element::Operator(String::new(), None),
                        "mn" => Element::Number(String::new(), None),
                        "math" | "mrow" => Element::Row(Vec::new()),
                        "msqrt" => Element::Sqrt(Box::new(Element::ParsePlaceholder)),
                        "mfrac" => Element::Fraction { numer: Box::new(Element::ParsePlaceholder), denom: Box::new(Element::ParsePlaceholder) },
                        "mroot" => Element::Root { base: Box::new(Element::ParsePlaceholder), index: Box::new(Element::ParsePlaceholder) },
                        "msub" => Element::Subscript { base: Box::new(Element::ParsePlaceholder), script: Box::new(Element::ParsePlaceholder) },
                        "msup" => Element::Superscript { base: Box::new(Element::ParsePlaceholder), script: Box::new(Element::ParsePlaceholder) },
                        "msubsup" => Element::Subsuperscript { base: Box::new(Element::ParsePlaceholder), subscript: Box::new(Element::ParsePlaceholder), superscript: Box::new(Element::ParsePlaceholder) },
                        _ => return Err(MathMLParseError::UnexpectedXMLTag(name.local_name))
                    });
                }
                XmlEvent::Characters(s) => {
                    els.last_mut().ok_or_else(|| MathMLParseError::UnexpectedXMLEvent(XmlEvent::Characters(s.clone())))
                        .and_then(|e| e.set_body(s, rx, fnt))?;
                }
                e@XmlEvent::EndElement { .. } => {
                    if els.len() == 0 {
                        return Err(MathMLParseError::UnexpectedXMLEvent(e));
                    } else if els.len() == 1 {
                        return Ok(els.pop().unwrap());
                    } else {
                        let el = els.pop().ok_or_else(|| MathMLParseError::UnexpectedXMLEvent(e))?;
                        els.last_mut().unwrap().append_child(el)?;
                    }
                }
                e => return Err(MathMLParseError::UnexpectedXMLEvent(e))
            }
        }
    }
    fn bounds(&self) -> Rect {
        match self {
            &Element::Id(_, ref ly) | &Element::Number(_, ref ly) | &Element::Operator(_, ref ly) => {
                ly.as_ref().map(|l| {
                    let b = l.bounds();
                    b.offset(Point::xy(0.0, b.h/2.0))
                }).unwrap()
            },
            &Element::Row(ref els) => {
                let (mut width, mut height) = (0.0, 0f32);
                for b in els.iter().map(|e| e.bounds()) {
                    width += b.x + b.w + 2.0;
                    height = height.max(b.h);
                }
                Rect::xywh(0.0, 0.0, width, height)
            },
            &Element::Fraction { ref numer, ref denom } => {
                let nb = numer.bounds();
                let db = denom.bounds();
                Rect::xywh(0.0, 0.0, nb.w.max(db.w), nb.h + db.h + 2.0)
            },
            &Element::Sqrt(ref c) => {
                let mut b = c.bounds();
                b.w += 10.0;
                b
            }
            &Element::Root { ref base, ref index } => {
                let mut b = base.bounds();
                let i = index.bounds();
                b.w += 7.0 + i.w;
                b
            }
            &Element::Subscript { ref base, ref script } => {
                let bb = base.bounds();
                let sb = script.bounds();
                union_rect(bb, sb.offset(Point::xy(bb.x+bb.w, bb.h+sb.h/2.0)))
            }
            &Element::Superscript { ref base, ref script } => {
                let bb = base.bounds();
                let sb = script.bounds();
                union_rect(bb, sb.offset(Point::xy(bb.x+bb.w, sb.h/2.0)))
            }
            &Element::Subsuperscript { ref base, ref subscript, ref superscript } => {
                let bb = base.bounds();
                let sub = subscript.bounds();
                let spb = superscript.bounds();
                union_rect(bb, union_rect(sub.offset(Point::xy(bb.x+bb.w, bb.h + sub.h/2.0)), 
                                          sub.offset(Point::xy(bb.x+bb.w, spb.h/2.0))))
            }
            _ => panic!("bounds for silly element")
        }
    }

    fn draw(&self, p: Point, rx: &mut RenderContext) {
        fn draw_radical(rx: &mut RenderContext, p: Point, eb: Rect, d: f32) {
            // draw radical sign
            let p1 = p + Point::xy(d, 0.0);
            let p2 = p + Point::xy(d+2.0, eb.h/2.0);
            let p3 = p + Point::xy(d+6.0, -eb.h/2.0);
            let p4 = p + Point::xy(d+8.0 + eb.w, -eb.h/2.0);
            let p5 = p + Point::xy(d+8.0 + eb.w, 5.0 -eb.h/2.0);
            rx.draw_line(p, p1, 1.0);
            rx.draw_line(p1, p2, 1.0);
            rx.draw_line(p2, p3, 1.0);
            rx.draw_line(p3, p4, 1.0);
            rx.draw_line(p4, p5, 1.0);
        }

        //rx.stroke_rect(self.bounds().offset(p), 1.0);

        match self {
            &Element::Id(_, ref ly) | &Element::Number(_, ref ly) | &Element::Operator(_, ref ly) => {
                let ly = ly.as_ref().unwrap();
                let b = ly.bounds();
                rx.draw_text_layout(p - Point::xy(0.0, b.h/2.0), ly);
            },
            &Element::Row(ref els) => {
                let mut pp = p;
                for e in els {
                    e.draw(pp, rx);
                    let eb = e.bounds();
                    pp.x += eb.x+eb.w+2.0;
                }
            },
            &Element::Fraction { ref numer, ref denom } => {
                let nb = numer.bounds();
                let db = denom.bounds();
                numer.draw(p - Point::xy(0.0, nb.h / 2.0 + 1.0), rx);
                rx.draw_line(p, p + Point::xy(nb.w.max(db.w), 0.0), 1.0);
                denom.draw(p + Point::xy(0.0, db.h / 2.0 + 1.0), rx);
            },
            &Element::Sqrt(ref el) => {
                let eb = el.bounds();
                draw_radical(rx, p, eb, 2.0);
                el.draw(p + Point::xy(9.0, 0.0), rx);
            },
            &Element::Root { ref base, ref index } => {
                let eb = base.bounds();
                let ib = index.bounds();
                draw_radical(rx, p, eb, ib.w);
                base.draw(p + Point::xy(ib.w+7.0, 0.0), rx);
                index.draw(p - Point::xy(0.0,ib.h/2.0), rx);
            }
            _ => panic!("draw silly element")
        }
    }
} 

pub struct MathExpression {
    root: Element
}

impl MathExpression {
    pub fn from_mathml<R: Read>(source: R, rx: &mut RenderContext, font: &Font) -> Result<MathExpression, MathMLParseError> {
        let mut parser = EventReader::new(source);
        match parser.next()? {
            XmlEvent::StartDocument { .. } => {},
            e => return Err(MathMLParseError::UnexpectedXMLEvent(e))
        };
        Ok(MathExpression { root: Element::from_mathml(&mut parser, rx, font)? })
    }

    pub fn draw(&self, p: Point, rx: &mut RenderContext) {
        self.root.draw(p, rx);
    }
}
