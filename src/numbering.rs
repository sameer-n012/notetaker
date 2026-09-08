/*
 * Manages the numbering state and Table of Contents collection.
 *
 * Precomputes a mapping of id -> number so @ref can properly resolve to a
 * number even if the referenced item comes later in the document.
 *
 * Numbering is hierarchichal and shared across all numbered items. Each
 * numbered blocks starts a new scope for its descendents. Unnumbered blocks
 * do not start new scopes.
 *
 * A Table of Contents entry is collected for every numbered block whose
 * label def sets `toc: true` (see `defs/section.def`).
*/

use crate::ast::Node;
use crate::defs::LabelMap;
use std::collections::HashMap;

pub struct TocEntry {
    pub number: String,
    pub title: String,
    pub id: Option<String>,
    pub children: Vec<TocEntry>,
}

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

/*
 * Walks the document tree computing numbers and inserts them into the
 * ids -> number mapping. Also inserts any ToC elements into a ToC tree.
 *
 * @param nodes: the document tree to walk
 * @param defs: the label definitions, used to determine which labels are
 * numbered
 * @param prefix: the current numbering prefix, e.g. [2, 1] for "2.1"
 * @param counter: the current counter for this scope, e.g. 1 for "2.1"
 * @param ids: the mapping of id -> number to populate
 *
 * @return: a vector of ToC entries for this scope
 */
fn collect(
    nodes: &[Node],
    defs: &LabelMap,
    prefix: &[u32],
    counter: &mut u32,
    ids: &mut HashMap<String, String>,
) -> Vec<TocEntry> {
    let mut toc = Vec::new();
    for node in nodes {
        if let Node::Block {
            label,
            args,
            id,
            children,
        } = node
        {
            if let Some(def) = defs.get(label).filter(|d| d.numbered) {
                *counter += 1;
                let mut path = prefix.to_vec();
                path.push(*counter);
                let number = format_number(&path);
                if let Some(id) = id {
                    ids.insert(id.clone(), number.clone());
                }
                let mut child_counter = 0u32;
                let child_toc = collect(children, defs, &path, &mut child_counter, ids);
                if def.toc {
                    toc.push(TocEntry {
                        number,
                        title: args.first().cloned().unwrap_or_default(),
                        id: id.clone(),
                        children: child_toc,
                    });
                } else {
                    toc.extend(child_toc);
                }
            } else {
                toc.extend(collect(children, defs, prefix, counter, ids));
            }
        }
    }
    toc
}

pub fn format_number(path: &[u32]) -> String {
    path.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defs::LabelDef;
    use crate::parser::parse;

    fn numbered(toc: bool) -> LabelDef {
        LabelDef {
            numbered: true,
            toc,
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
            defs.insert(label.to_string(), numbered(false));
        }

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
        defs.insert("section".to_string(), numbered(true));
        defs.insert("subsection".to_string(), numbered(true));
        defs.insert("theorem".to_string(), numbered(false));

        let counters = Counters::build(&doc.nodes, &defs);
        assert_eq!(counters.toc.len(), 2);

        assert_eq!(counters.toc[0].number, "1");
        assert_eq!(counters.toc[0].title, "Intro");
        assert_eq!(counters.toc[0].id.as_deref(), Some("s1"));
        assert_eq!(counters.toc[0].children.len(), 1);
        assert_eq!(counters.toc[0].children[0].number, "1.2");
        assert_eq!(counters.toc[0].children[0].title, "Background");

        assert_eq!(counters.toc[1].number, "2");
        assert_eq!(counters.toc[1].title, "Methods");
    }

    #[test]
    fn toc_membership_is_driven_by_the_toc_property_not_the_label_name() {
        let src = "chapter(One) #c1 {\n}\n\nsection(Two) #s1 {\n}\n";
        let doc = parse(src);

        let mut defs = LabelMap::new();
        defs.insert("chapter".to_string(), numbered(true));
        defs.insert("section".to_string(), numbered(false));

        let counters = Counters::build(&doc.nodes, &defs);
        assert_eq!(counters.toc.len(), 1);
        assert_eq!(counters.toc[0].title, "One");
        assert_eq!(counters.ids.get("c1").map(String::as_str), Some("1"));
        assert_eq!(counters.ids.get("s1").map(String::as_str), Some("2"));
    }
}
