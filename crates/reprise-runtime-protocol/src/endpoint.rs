//! Where the runtime answers.
//!
//! The bus name, object path and interface name are as much a part of the
//! contract as the field names are: a client that has the right types and
//! the wrong address talks to nothing. They live here, with the types, so
//! there is exactly one definition — and so a client does not have to depend
//! on the *service* crate just to learn an address, which on Linux would
//! drag GStreamer into every surface that only wanted to send a command.

/// The well-known name the runtime owns, and the name whose activation
/// starts it.
pub const BUS_NAME: &str = "org.reprise.Reprise1";

/// The object the interface lives at.
pub const OBJECT_PATH: &str = "/org/reprise/Reprise1";

/// The interface every command and every delta belongs to.
pub const INTERFACE_NAME: &str = "org.reprise.Reprise1";
