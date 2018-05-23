
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

fn check_end_element<R: Read>(reader: &mut EventReader<R>, s: &String) -> Result<(), MathMLParseError> {
    match reader.next()? {
        XmlEvent::EndElement { name } => {
            if name.local_name == *s {
                Ok(())
            } else {
                Err(MathMLParseError::UnexpectedXMLTag(name.local_name))
            }
        },
        e => Err(MathMLParseError::UnexpectedXMLEvent(e))
    }
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
            &mut Element::Id(ref mut body, ref mut layout) => { *body = s; }
            &mut Element::Number(ref mut body, ref mut layout) => { *body = s; }
            &mut Element::Operator(ref mut body, ref mut layout) => { *body = s; }
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
    
    fn children_from_mathml<R: Read>(reader: &mut EventReader<R>, rx: &mut RenderContext, fnt: &Font, name: &OwnedName) -> Result<Vec<Element>, MathMLParseError> {
        let mut elm = Vec::new();
        'm: loop {
            match Element::from_mathml(reader, rx, fnt) {
                Ok(e) => elm.push(e),
                Err(MathMLParseError::UnexpectedXMLEvent(XmlEvent::EndElement{ name: n })) => {
                    if n == *name {
                        break 'm
                    } else {
                        panic!("heck!");
                    }
                }
                Err(e) => return Err(e)
            }
        }
        Ok(elm)
    }

    fn from_mathml<R: Read>(reader: &mut EventReader<R>, rx: &mut RenderContext, fnt: &Font) -> Result<Element, MathMLParseError> {
        match reader.next()? {
            XmlEvent::StartElement { name, .. } => {
                match name.local_name.as_str() {
                    "mi" => {
                        match reader.next()? {
                            XmlEvent::Characters(s) => {
                                check_end_element(reader, &name.local_name).and(Ok(Element::Id(s, None)))
                            }
                            e => Err(MathMLParseError::UnexpectedXMLEvent(e))
                        }
                    }
                    "mo" => {
                        match reader.next()? {
                            XmlEvent::Characters(s) => {
                                check_end_element(reader, &name.local_name).and(Ok(Element::Operator(s, None)))
                            }
                            e => Err(MathMLParseError::UnexpectedXMLEvent(e))
                        }
                    }
                    "mn" => {
                        match reader.next()? {
                            XmlEvent::Characters(s) => {
                                check_end_element(reader, &name.local_name).and(Ok(Element::Number(s, None)))
                            }
                            e => Err(MathMLParseError::UnexpectedXMLEvent(e))
                        }
                    }
                    "math" | "mrow" => {
                        Element::children_from_mathml(reader, rx, fnt, &name).map(|es| Element::Row(es))
                    }
                    _ => Err(MathMLParseError::UnexpectedXMLTag(name.local_name))
                }
            },
            e => Err(MathMLParseError::UnexpectedXMLEvent(e))
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
    }
}
