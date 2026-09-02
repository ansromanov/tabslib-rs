//! GPIF codec: the XML payload inside a `.gp` container.

mod dom;
mod parse;
mod write;

pub use dom::{parse_xml, Element};
pub use parse::parse;
pub use write::write;
