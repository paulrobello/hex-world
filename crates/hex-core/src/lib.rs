//! Hex World core: hex grid math, mesh data, tile rules and the world model. No engine dependencies.

pub mod axial;
pub mod boundary;
pub mod catalog;
pub mod path;
pub mod prism;
pub mod rail_end;
pub mod save;
pub mod slope;
pub mod socket;
pub mod tile;
pub mod tunnel;
pub mod variant;
pub mod world;
pub use axial::*;
pub use catalog::*;
pub use prism::*;
pub use save::*;
pub use slope::*;
pub use socket::*;
pub use tile::*;
pub use tunnel::*;
pub use variant::*;
pub use world::*;
