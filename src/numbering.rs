use crate::ast::Node;
use crate::defs::LabelMap;
use std::collections::HashMap;

/// Precomputed `id -> number` map (e.g. `"cs" -> "2.2"`), so `@ref` can
/// resolve to a number regardless of whether the referenced block comes
/// later in the document.
///
/// Numbering is hierarchical and shared across every numbered label: all
/// numbered blocks at the same nesting scope share one counter (a
/// `definition` and a `theorem` right after it get 2.1 and 2.2, not
/// independent per-label counts), a numbered block starts a fresh child
/// scope for its own numbered descendants, and an unnumbered wrapper block
/// is transparent — its numbered children still count against its closest
/// numbered ancestor's scope as if the wrapper weren't there.
/// One entry in the table of contents (currently `section`/`subsection`
/// only — see `TOC_LABELS`), nested to match the document's structure.
pub struct TocEntry {
    pub number: String,
    pub title: String,
    pub id: Option<String>,
    pub children: Vec<TocEntry>,
}

const TOC_LABELS: [&str; 2] = ["section", "subsection"];

pub struct Counters {
    pub ids: HashMap<String, String>,
    pub toc: Vec<TocEntry>,
}

impl Counters {
    pub fn build(nodes: &[Node], defs: &LabelMap) -> Self {
        let mut ids = HashMap::new();
        let mut counter = 0u32;
        let toc = collect(nodes, defs, &[], &mut counter, &mut ids);
        Counters { ids, toc }
    }
}

/// Walks the tree computing numbers (same shared/hierarchical/transparent-
/// wrapper rules as the doc comment on `Counters` describes) and, as it
/// goes, also collects `section`/`subsection` entries into a ToC tree — the
/// same pass, so the ToC's numbers can never drift from the ones actually
/// rendered next to each heading.
fn collect(
    nodes: &[Node],
    defs: &LabelMap,
    prefix: &[u32],
    counter: &mut u32,
    ids: &mut HashMap<String, String>,
) -> Vec<TocEntry> {
    let mut toc = Vec::new();
    for node in nodes {
        if let Node::Block { label, args, id, children } = node {
            if defs.get(label).is_some_and(|d| d.numbered) {
                *counter += 1;
                let mut path = prefix.to_vec();
                path.push(*counter);
                let number = format_number(&path);
                if let Some(id) = id {
                    ids.insert(id.clone(), number.clone());
                }
                let mut child_counter = 0u32;
                let child_toc = collect(children, defs, &path, &mut child_counter, ids);
                if TOC_LABELS.contains(&label.as_str()) {
                    toc.push(TocEntry {
                        number,
                        title: args.first().cloned().unwrap_or_default(),
                        id: id.clone(),
                        children: child_toc,
                    });
                } else {
                    // not a heading itself, but any section/subsection found
                    // further inside it still belongs in the ToC
                    toc.extend(child_toc);
                }
            } else {
                // unnumbered wrapper: transparent, shares the current scope
                toc.extend(collect(children, defs, prefix, counter, ids));
            }
        }
    }
    toc
}

pub fn format_number(path: &[u32]) -> String {
    path.iter().map(u32::to_string).collect::<Vec<_>>().join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defs::LabelDef;
    use crate::parser::parse;

    fn numbered() -> LabelDef {
        LabelDef {
            numbered: true,
            latex_template: String::new(),
            html_template: String::new(),
            style: None,
        }
    }

    #[test]
    fn hierarchical_numbering_matches_spec_example() {
        // section #s1 {}                     -> 1
        // section #s2 {                      -> 2
        //   unnumbered_item {                   (transparent, no number)
        //     definition #d {}               -> 2.1
        //     theorem #t {                   -> 2.2
        //       lemma #l {}                  -> 2.2.1
        //     }
        //   }
        // }
        let src = "section #s1 {\n}\n\nsection #s2 {\nunnumbered_item {\ndefinition #d {\n}\ntheorem #t {\nlemma #l {\n}\n}\n}\n}\n";
        let doc = parse(src);

        let mut defs = LabelMap::new();
        for label in ["section", "definition", "theorem", "lemma"] {
            defs.insert(label.to_string(), numbered());
        }
        // `unnumbered_item` deliberately has no entry in `defs` at all, same
        // as any label a user hasn't wired up a .def file for.

        let counters = Counters::build(&doc.nodes, &defs);
        assert_eq!(counters.ids.get("s1").map(String::as_str), Some("1"));
        assert_eq!(counters.ids.get("s2").map(String::as_str), Some("2"));
        assert_eq!(counters.ids.get("d").map(String::as_str), Some("2.1"));
        assert_eq!(counters.ids.get("t").map(String::as_str), Some("2.2"));
        assert_eq!(counters.ids.get("l").map(String::as_str), Some("2.2.1"));
    }

    #[test]
    fn toc_nests_subsections_under_their_section_and_skips_other_labels() {
        let src = "\
section(Intro) #s1 {\n\
theorem #t {\n}\n\
subsection(Background) #sub1 {\n}\n\
}\n\
section(Methods) #s2 {\n}\n";
        let doc = parse(src);

        let mut defs = LabelMap::new();
        for label in ["section", "subsection", "theorem"] {
            defs.insert(label.to_string(), numbered());
        }

        let counters = Counters::build(&doc.nodes, &defs);
        assert_eq!(counters.toc.len(), 2);

        assert_eq!(counters.toc[0].number, "1");
        assert_eq!(counters.toc[0].title, "Intro");
        assert_eq!(counters.toc[0].id.as_deref(), Some("s1"));
        // theorem #t is numbered but not a heading, so it doesn't appear in the ToC
        assert_eq!(counters.toc[0].children.len(), 1);
        assert_eq!(counters.toc[0].children[0].number, "1.2");
        assert_eq!(counters.toc[0].children[0].title, "Background");

        assert_eq!(counters.toc[1].number, "2");
        assert_eq!(counters.toc[1].title, "Methods");
    }
}
