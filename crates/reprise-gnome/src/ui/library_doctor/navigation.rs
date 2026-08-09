use std::rc::Rc;

use libadwaita as adw;

use super::review_page::LibraryDoctorReviewPage;

const ROOT_TAG: &str = "library-doctor";
const REVIEW_TAG: &str = "library-doctor-review";
#[derive(Clone)]
pub(super) struct DoctorNavigation {
    content_navigation: adw::NavigationView,
    content_stack: gtk4::Stack,
    doctor_navigation: adw::NavigationView,
}
impl DoctorNavigation {
    pub(super) fn new(
        content_navigation: &adw::NavigationView,
        content_stack: &gtk4::Stack,
        doctor_navigation: &adw::NavigationView,
    ) -> Self {
        Self {
            content_navigation: content_navigation.clone(),
            content_stack: content_stack.clone(),
            doctor_navigation: doctor_navigation.clone(),
        }
    }

    pub(super) fn add_root(&self, page: &adw::NavigationPage) {
        self.doctor_navigation.add(page);
    }

    pub(super) fn show_root(&self) {
        self.show_content();
        if let Some(page) = self.doctor_navigation.find_page(ROOT_TAG) {
            self.doctor_navigation.pop_to_page(&page);
        }
    }

    /// Shows exactly the page it was handed.
    ///
    /// Every review page carries the same tag, and the view refuses a second
    /// page with a tag it already knows. Walking back to whatever holds the tag
    /// is only right while that is this very page; for a page from an earlier
    /// scan it would show yesterday's findings and let Apply write yesterday's
    /// plan. Popping the old one is what removes it — it was pushed, never
    /// added — so the new page can take the tag.
    pub(super) fn show_review(&self, page: &adw::NavigationPage) {
        self.show_content();
        match self.doctor_navigation.find_page(REVIEW_TAG) {
            Some(shown) if &shown == page => {
                self.doctor_navigation.pop_to_tag(REVIEW_TAG);
            }
            Some(_) => {
                self.doctor_navigation.pop_to_tag(ROOT_TAG);
                self.doctor_navigation.push(page);
            }
            None => self.doctor_navigation.push(page),
        }
    }

    pub(super) fn show_review_or_root(&self, review: Option<Rc<LibraryDoctorReviewPage>>) {
        if let Some(review) = review {
            self.show_review(review.navigation_page());
        } else {
            self.show_root();
        }
    }

    pub(super) fn is_visible(&self) -> bool {
        self.content_stack.visible_child_name().as_deref() == Some(ROOT_TAG)
    }

    fn show_content(&self) {
        if let Some(root) = self
            .content_navigation
            .find_page(super::super::now_playing_wiring::LIBRARY_CONTENT_TAG)
        {
            self.content_navigation.pop_to_page(&root);
        }
        super::super::window::content_stack::show_page(&self.content_stack, ROOT_TAG);
    }
}
