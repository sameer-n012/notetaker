#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Paragraph(Vec<Inline>),
    Code {
        lang: Option<String>,
        source: String,
    },
    Block {
        label: String,
        args: Vec<String>,
        id: Option<String>,
        children: Vec<Node>,
    },
}

// `MathInline` ends with the enum name, but it pairs with `MathDisplay`; a
// shorter name such as `Math` would hide which of the two math forms it is.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    MathInline(String),
    MathDisplay(String),
    Ref(String),
}
