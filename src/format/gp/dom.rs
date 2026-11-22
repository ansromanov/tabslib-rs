//! A minimal read-only XML tree. GPIF is small and regular enough that a full
//! serde layer buys nothing, and a hand-rolled tree keeps attribute access
//! explicit -- which matters here, because several GPIF features live in
//! `<Property name="...">` attributes rather than element names.

use crate::error::{Error, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

/// Limits applied while building the in-memory GPIF XML tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XmlLimits {
    /// Maximum UTF-8 payload size in bytes.
    pub max_xml_bytes: usize,
    /// Maximum nesting depth.
    pub max_depth: usize,
    /// Maximum number of elements.
    pub max_nodes: usize,
}

/// Conservative defaults for untrusted GPIF XML.
pub const DEFAULT_XML_LIMITS: XmlLimits = XmlLimits {
    max_xml_bytes: 128 * 1024 * 1024,
    max_depth: 1024,
    max_nodes: 2_000_000,
};

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
    parse_xml_with_limits(xml, DEFAULT_XML_LIMITS)
}

/// Parses GPIF XML using caller-supplied size, depth, and node limits.
pub fn parse_xml_with_limits(xml: &str, limits: XmlLimits) -> Result<Element> {
    if xml.len() > limits.max_xml_bytes {
        return Err(Error::LimitExceeded {
            resource: "GPIF XML bytes",
            limit: limits.max_xml_bytes,
        });
    }
    if limits.max_depth == 0 || limits.max_nodes == 0 {
        return Err(Error::LimitExceeded {
            resource: "GPIF XML",
            limit: 0,
        });
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Element> = vec![Element {
        name: "#root".into(),
        ..Default::default()
    }];
    let mut nodes = 0usize;
    loop {
        match reader.read_event().map_err(|e| Error::format("gp", e))? {
            Event::Start(e) => {
                nodes = nodes.saturating_add(1);
                if stack.len() > limits.max_depth {
                    return Err(Error::LimitExceeded {
                        resource: "GPIF XML depth",
                        limit: limits.max_depth,
                    });
                }
                if nodes > limits.max_nodes {
                    return Err(Error::LimitExceeded {
                        resource: "GPIF XML nodes",
                        limit: limits.max_nodes,
                    });
                }
                let mut el = Element {
                    name: String::from_utf8_lossy(e.name().as_ref()).into_owned(),
                    ..Default::default()
                };
                for a in e.attributes() {
                    let a = a.map_err(|error| Error::format("gp", error))?;
                    el.attrs.push((
                        String::from_utf8_lossy(a.key.as_ref()).into_owned(),
                        String::from_utf8_lossy(&a.value).into_owned(),
                    ));
                }
                stack.push(el);
            }
            Event::Empty(e) => {
                nodes = nodes.saturating_add(1);
                if nodes > limits.max_nodes {
                    return Err(Error::LimitExceeded {
                        resource: "GPIF XML nodes",
                        limit: limits.max_nodes,
                    });
                }
                let mut el = Element {
                    name: String::from_utf8_lossy(e.name().as_ref()).into_owned(),
                    ..Default::default()
                };
                for a in e.attributes() {
                    let a = a.map_err(|error| Error::format("gp", error))?;
                    el.attrs.push((
                        String::from_utf8_lossy(a.key.as_ref()).into_owned(),
                        String::from_utf8_lossy(&a.value).into_owned(),
                    ));
                }
                stack
                    .last_mut()
                    .ok_or_else(|| Error::Malformed("XML stack underflow".into()))?
                    .children
                    .push(el);
            }
            Event::Text(t) => {
                let s = t.unescape().map_err(|error| Error::format("gp", error))?;
                if !s.trim().is_empty() {
                    stack
                        .last_mut()
                        .ok_or_else(|| Error::Malformed("XML stack underflow".into()))?
                        .text
                        .push_str(&s);
                }
            }
            Event::CData(t) => {
                let s = String::from_utf8_lossy(&t.into_inner()).into_owned();
                stack
                    .last_mut()
                    .ok_or_else(|| Error::Malformed("XML stack underflow".into()))?
                    .text
                    .push_str(&s);
            }
            Event::End(_) => {
                if stack.len() <= 1 {
                    return Err(Error::Malformed("unexpected XML closing element".into()));
                }
                let done = stack
                    .pop()
                    .ok_or_else(|| Error::Malformed("XML stack underflow".into()))?;
                stack
                    .last_mut()
                    .ok_or_else(|| Error::Malformed("XML stack underflow".into()))?
                    .children
                    .push(done);
            }
            Event::Eof => {
                if stack.len() != 1 {
                    return Err(Error::Malformed("unclosed XML element".into()));
                }
                break;
            }
            _ => {}
        }
    }
    let root = stack
        .pop()
        .ok_or_else(|| Error::Malformed("XML stack underflow".into()))?;
    Ok(root
        .children
        .into_iter()
        .find(|c| c.name == "GPIF")
        .unwrap_or_default())
}

#[cfg(test)]
#[path = "dom_test.rs"]
mod tests;
