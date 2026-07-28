#![allow(dead_code)]

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::formatted;

pub const SOURCE_ADD: &str = N_!("Add");
pub const SOURCE_ADDED: &str = N_!("Added");
pub const SOURCE_SUBSCRIBE_ACCESSIBLE: &str = N_!("Subscribe to {source}");
pub const SOURCE_ADD_ACCESSIBLE: &str = N_!("Add {source}");
pub const SOURCE_ADDED_ACCESSIBLE: &str = N_!("{source} is already in your library");
pub const SOURCE_SUBSCRIBED_DROP_OUT: &str = N_!("Subscribed sources drop out of later searches.");

pub fn source_subscribe_accessible(source: &str) -> String {
    formatted(SOURCE_SUBSCRIBE_ACCESSIBLE, &[("source", source)])
}

pub fn source_add_accessible(source: &str) -> String {
    formatted(SOURCE_ADD_ACCESSIBLE, &[("source", source)])
}

pub fn source_added_accessible(source: &str) -> String {
    formatted(SOURCE_ADDED_ACCESSIBLE, &[("source", source)])
}
