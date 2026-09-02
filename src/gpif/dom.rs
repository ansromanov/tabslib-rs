//! A minimal read-only XML tree. GPIF is small and regular enough that a full
//! serde layer buys nothing, and a hand-rolled tree keeps attribute access
//! explicit -- which matters here, because several GPIF features live in
//! `<Property name="...">` attributes rather than element names.

use crate::error::Result;
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, Default, Clone)]
/// One XML element, with its attributes, text and children.
pub struct Element {
    /// Tag name.
    pub name: String,
    /// Attributes in document order.
    pub attrs: Vec<(String, String)>,
    /// Concatenated text and CDATA content.
    pub text: String,
    /// Child elements in document order.
    pub children: Vec<Element>,
}

impl Element {
    /// Attribute value by name.
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
    /// First child with this tag name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }
    /// Every child with this tag name.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |c| c.name == name)
    }
    /// Trimmed text of the first child with this tag name, if non-empty.
    pub fn child_text(&self, name: &str) -> Option<&str> {
        self.child(name)
            .map(|c| c.text.trim())
            .filter(|t| !t.is_empty())
    }
    /// A GPIF `<Property name="X">` lookup. The previous engine matched element
    /// names here and so could never see palm mutes, slides or bends at all.
    pub fn property(&self, name: &str) -> Option<&Element> {
        self.child("Properties")
            .into_iter()
            .flat_map(|p| p.children_named("Property"))
            .find(|p| p.attr("name") == Some(name))
    }
    /// Whether a `<Property name="...">` is present.
    pub fn has_property(&self, name: &str) -> bool {
        self.property(name).is_some()
    }
}

/// Parses a GPIF document into a tree rooted at `<GPIF>`.
pub fn parse_xml(xml: &str) -> Result<Element> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Element> = vec![Element {
        name: "#root".into(),
        ..Default::default()
    }];
    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let mut el = Element {
                    name: String::from_utf8_lossy(e.name().as_ref()).into_owned(),
                    ..Default::default()
                };
                for a in e.attributes().flatten() {
                    el.attrs.push((
                        String::from_utf8_lossy(a.key.as_ref()).into_owned(),
                        String::from_utf8_lossy(&a.value).into_owned(),
                    ));
                }
                stack.push(el);
            }
            Event::Empty(e) => {
                let mut el = Element {
                    name: String::from_utf8_lossy(e.name().as_ref()).into_owned(),
                    ..Default::default()
                };
                for a in e.attributes().flatten() {
                    el.attrs.push((
                        String::from_utf8_lossy(a.key.as_ref()).into_owned(),
                        String::from_utf8_lossy(&a.value).into_owned(),
                    ));
                }
                stack.last_mut().unwrap().children.push(el);
            }
            Event::Text(t) => {
                let s = t.unescape().unwrap_or_default();
                if !s.trim().is_empty() {
                    stack.last_mut().unwrap().text.push_str(&s);
                }
            }
            Event::CData(t) => {
                let s = String::from_utf8_lossy(&t.into_inner()).into_owned();
                stack.last_mut().unwrap().text.push_str(&s);
            }
            Event::End(_) => {
                if stack.len() > 1 {
                    let done = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(done);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let root = stack.pop().unwrap();
    Ok(root
        .children
        .into_iter()
        .find(|c| c.name == "GPIF")
        .unwrap_or_default())
}
