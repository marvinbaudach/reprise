//! Shared scaffolding for the "add a source" dialogs (Podcasts/YouTube and Radio): the dialog
//! chrome every source builds the same way, and the request generation that drops stale async
//! results. The phase model, the result list and the preview stay per source.
pub(in crate::ui) mod chrome;
pub(in crate::ui) mod generation;
#[cfg(test)]
pub(in crate::ui) mod test_support;
