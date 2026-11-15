//! Clean-room Advance Wars By Web rules engine.
//!
//! Builds without std (`default-features = false`) for zkVM guests that have only `alloc`.

#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

/// What std's prelude would bring in, for a crate that is `no_std`.
pub(crate) mod prelude {
    pub use alloc::format;
    pub use alloc::string::{String, ToString};
    pub use alloc::vec;
    pub use alloc::vec::Vec;
}


pub mod actions;
pub mod awbw;
pub mod baseline;
pub mod co;
pub mod combat;
pub mod data;
pub mod fog;
pub mod fx;
pub mod num;
pub mod rules;
pub mod sim;
pub mod state;
pub mod storyboard;
pub mod view;

pub use state::*;
