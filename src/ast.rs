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

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    MathInline(String),
    MathDisplay(String),
    Ref(String),
}
