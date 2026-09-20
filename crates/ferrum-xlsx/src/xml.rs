//! Writing XML.
//!
//! Enough for the parts of a spreadsheet file, and no more. The one thing it
//! is strict about is escaping, because that is where a hand-rolled writer
//! produces a file that opens fine until somebody types an ampersand.

/// Builds an XML document.
pub struct XmlWriter {
    out: String,
    /// Names of the elements currently open, so a close can be checked.
    open: Vec<String>,
}

impl Default for XmlWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl XmlWriter {
    /// Start a document with the declaration these files carry.
    pub fn new() -> Self {
        Self {
            out: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n"),
            open: Vec::new(),
        }
    }

    /// Start without a declaration, for a fragment.
    pub fn fragment() -> Self {
        Self {
            out: String::new(),
            open: Vec::new(),
        }
    }

    /// Open an element with attributes, leaving it open for children.
    pub fn start(&mut self, name: &str, attributes: &[(&str, &str)]) -> &mut Self {
        self.out.push('<');
        self.out.push_str(name);
        self.write_attributes(attributes);
        self.out.push('>');
        self.open.push(name.to_string());
        self
    }

    /// Write an element with no children.
    pub fn empty(&mut self, name: &str, attributes: &[(&str, &str)]) -> &mut Self {
        self.out.push('<');
        self.out.push_str(name);
        self.write_attributes(attributes);
        self.out.push_str("/>");
        self
    }

    /// Write an element whose only content is text.
    pub fn text_element(
        &mut self,
        name: &str,
        attributes: &[(&str, &str)],
        text: &str,
    ) -> &mut Self {
        self.start(name, attributes);
        self.text(text);
        self.end(name)
    }

    /// Write character data, escaped.
    pub fn text(&mut self, text: &str) -> &mut Self {
        escape_into(&mut self.out, text, false);
        self
    }

    /// Close the most recently opened element.
    ///
    /// The name is required and checked. Mismatched nesting is a bug that
    /// otherwise shows up as a file a reader rejects with no clue why.
    pub fn end(&mut self, name: &str) -> &mut Self {
        match self.open.pop() {
            Some(open) => debug_assert_eq!(
                open, name,
                "closing <{name}> but <{open}> is the one that is open"
            ),
            None => debug_assert!(false, "closing <{name}> with nothing open"),
        }
        self.out.push_str("</");
        self.out.push_str(name);
        self.out.push('>');
        self
    }

    fn write_attributes(&mut self, attributes: &[(&str, &str)]) {
        for (name, value) in attributes {
            self.out.push(' ');
            self.out.push_str(name);
            self.out.push_str("=\"");
            escape_into(&mut self.out, value, true);
            self.out.push('"');
        }
    }

    /// How many elements are still open. Zero means the document is balanced.
    pub fn depth(&self) -> usize {
        self.open.len()
    }

    /// Finish and hand back the document.
    pub fn finish(self) -> String {
        debug_assert_eq!(self.open.len(), 0, "the document has unclosed elements");
        self.out
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.finish().into_bytes()
    }
}

/// Escape text for XML.
///
/// `&` and `<` always, `>` because a bare one inside `]]>` is invalid and
/// escaping it unconditionally is cheaper than looking, and the quotes only
/// inside an attribute. Characters XML cannot carry at all are dropped rather
/// than written, because a control byte in a cell would make the whole file
/// unreadable.
fn escape_into(out: &mut String, text: &str, in_attribute: bool) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if in_attribute => out.push_str("&quot;"),
            '\'' if in_attribute => out.push_str("&apos;"),
            // Tab, newline and carriage return are the only control
            // characters XML 1.0 allows.
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
}

/// Escape text for XML, as a standalone helper.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    escape_into(&mut out, text, false);
    out
}

/// Escape text for an XML attribute value.
pub fn escape_attribute(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    escape_into(&mut out, text, true);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_starts_with_the_declaration() {
        let mut xml = XmlWriter::new();
        xml.empty("root", &[]);
        let out = xml.finish();
        assert!(out.starts_with("<?xml version=\"1.0\""));
        assert!(out.ends_with("<root/>"));
    }

    #[test]
    fn elements_nest() {
        let mut xml = XmlWriter::fragment();
        xml.start("a", &[]);
        xml.start("b", &[("x", "1")]);
        xml.text("hello");
        xml.end("b");
        xml.end("a");
        assert_eq!(xml.finish(), r#"<a><b x="1">hello</b></a>"#);
    }

    #[test]
    fn attributes_are_written_in_order() {
        let mut xml = XmlWriter::fragment();
        xml.empty("c", &[("r", "A1"), ("t", "s"), ("s", "0")]);
        assert_eq!(xml.finish(), r#"<c r="A1" t="s" s="0"/>"#);
    }

    #[test]
    fn the_markup_characters_are_escaped_in_text() {
        let mut xml = XmlWriter::fragment();
        xml.text_element("v", &[], "a & b < c > d");
        assert_eq!(xml.finish(), "<v>a &amp; b &lt; c &gt; d</v>");
    }

    #[test]
    fn quotes_are_escaped_only_inside_an_attribute() {
        assert_eq!(escape(r#"say "hi""#), r#"say "hi""#);
        assert_eq!(escape_attribute(r#"say "hi""#), "say &quot;hi&quot;");
        assert_eq!(escape_attribute("it's"), "it&apos;s");
    }

    #[test]
    fn a_formula_full_of_markup_survives() {
        // The case a naive writer gets wrong and nobody notices until a user
        // types a comparison into a cell.
        let mut xml = XmlWriter::fragment();
        xml.text_element("f", &[], r#"IF(A1<B1,"x & y",C1>0)"#);
        assert_eq!(xml.finish(), r#"<f>IF(A1&lt;B1,"x &amp; y",C1&gt;0)</f>"#);
    }

    #[test]
    fn control_characters_are_dropped_rather_than_written() {
        // A NUL in a cell would otherwise make the whole file unreadable.
        let mut xml = XmlWriter::fragment();
        xml.text_element("v", &[], "a\u{0}b\u{7}c");
        assert_eq!(xml.finish(), "<v>abc</v>");
    }

    #[test]
    fn the_three_allowed_control_characters_survive() {
        let mut xml = XmlWriter::fragment();
        xml.text_element("v", &[], "a\tb\nc\rd");
        assert_eq!(xml.finish(), "<v>a\tb\nc\rd</v>");
    }

    #[test]
    fn text_outside_the_basic_plane_survives() {
        let mut xml = XmlWriter::fragment();
        xml.text_element("v", &[], "naïve 日本語 🧮");
        assert_eq!(xml.finish(), "<v>naïve 日本語 🧮</v>");
    }

    #[test]
    fn depth_reports_what_is_still_open() {
        let mut xml = XmlWriter::fragment();
        assert_eq!(xml.depth(), 0);
        xml.start("a", &[]);
        assert_eq!(xml.depth(), 1);
        xml.start("b", &[]);
        assert_eq!(xml.depth(), 2);
        xml.end("b");
        xml.end("a");
        assert_eq!(xml.depth(), 0);
    }

    #[test]
    #[should_panic(expected = "is the one that is open")]
    fn closing_the_wrong_element_is_caught_in_a_debug_build() {
        let mut xml = XmlWriter::fragment();
        xml.start("a", &[]);
        xml.end("b");
    }
}
