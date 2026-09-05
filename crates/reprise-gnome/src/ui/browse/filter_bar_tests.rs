use std::cell::RefCell;
use std::rc::Rc;

use reprise_view::search_scope::SearchScope;

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TestFilter {
    query: String,
    selections: Vec<(String, String)>,
}

#[derive(Default)]
struct TestModel;

impl FilterModel for TestModel {
    type Filter = TestFilter;

    fn initial_filter(&self) -> Self::Filter {
        TestFilter::default()
    }

    fn facets(&self) -> Vec<FacetDescriptor> {
        vec![
            FacetDescriptor::single("kind", "Kind"),
            FacetDescriptor::single("place", "Place"),
        ]
    }

    fn values(&self, facet_id: &str) -> Vec<ValueDescriptor> {
        match facet_id {
            "kind" => vec![ValueDescriptor::new("album", "Album")],
            "place" => vec![ValueDescriptor::new("ch", "Switzerland")],
            _ => Vec::new(),
        }
    }

    fn apply(&self, query: &str, selections: &[(String, String)]) -> Self::Filter {
        TestFilter {
            query: query.to_owned(),
            selections: selections.to_vec(),
        }
    }

    fn persistence_key(&self) -> &'static str {
        "test.filter"
    }

    fn query<'a>(&self, filter: &'a Self::Filter) -> &'a str {
        &filter.query
    }

    fn selections(&self, filter: &Self::Filter) -> Vec<SelectionDescriptor> {
        filter
            .selections
            .iter()
            .map(|(facet, value)| {
                let label = self
                    .values(facet)
                    .into_iter()
                    .find(|candidate| candidate.id == *value)
                    .map_or_else(|| value.clone(), |candidate| candidate.label);
                SelectionDescriptor::new(facet.clone(), value.clone(), label)
            })
            .collect()
    }

    fn search_scope(&self) -> SearchScope {
        SearchScope::Releases
    }

    fn add_filter_label(&self) -> String {
        "Add filter".into()
    }

    fn clear_all_label(&self) -> String {
        "Clear all".into()
    }

    fn count_text(&self, shown: usize, _total: usize, _active: bool) -> CountText {
        CountText::plain(match shown {
            0 => "No items".into(),
            1 => "1 item".into(),
            count => format!("{count} items"),
        })
    }
}

fn bar() -> Rc<FilterBar<TestModel>> {
    FilterBar::new(TestModel)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn committed_query_round_trips_and_escape_uses_the_section_clear_path() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = bar();
    let requests = Rc::new(RefCell::new(Vec::new()));
    bar.set_on_query_changed({
        let requests = requests.clone();
        let bar = Rc::downgrade(&bar);
        move |query| {
            requests.borrow_mut().push(query.to_owned());
            if let Some(bar) = bar.upgrade() {
                bar.set_query(query);
                bar.set_committed_query(query);
            }
        }
    });

    bar.set_query("needle");
    bar.set_committed_query("needle");
    assert_eq!(bar.committed_query(), "needle");
    bar.request_search_clear();

    assert_eq!(&*requests.borrow(), &[String::new()]);
    assert_eq!(bar.filter().query, "");
    assert_eq!(bar.committed_query(), "");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn chip_add_remove_and_clear_all_reset_the_whole_section() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = bar();

    bar.select("kind", "album");
    assert_eq!(bar.filter().selections, [("kind".into(), "album".into())]);
    assert_eq!(bar.chip_labels(), ["Album"]);

    bar.remove("kind", "album");
    assert!(bar.filter().selections.is_empty());
    assert!(bar.chip_labels().is_empty());

    bar.set_query("needle");
    bar.set_committed_query("needle");
    bar.select("place", "ch");
    bar.clear_all();
    assert_eq!(bar.filter(), TestFilter::default());
    assert!(bar.chip_labels().is_empty());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn count_line_names_zero_one_and_many() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = bar();

    bar.set_counts(0, 3);
    assert_eq!(bar.count_text(), "No items");
    bar.set_counts(1, 3);
    assert_eq!(bar.count_text(), "1 item");
    bar.set_counts(3, 3);
    assert_eq!(bar.count_text(), "3 items");
}
