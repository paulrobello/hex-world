//! Hex World simulation core (PRD §7): stitched network graphs, routing and cars on a fixed 20 Hz
//! tick. Pure Rust with no engine dependency, so it survives a fallback off Bevy (PRD §12.1, D2).
//!
//! The app mirrors every world edit with [`Simulation::apply_place`] and
//! [`Simulation::apply_bulldoze`], runs [`Simulation::tick`] at [`TICK_HZ`], and draws each car at
//! [`Car::pose_at`] with the fraction of a tick elapsed since the last one. Parked cars
//! ([`Simulation::parked`]) stand still at [`ParkedCar::pose`] between trips (PRD §7.1).

pub mod geom;
pub mod network;
pub mod rail;
pub mod route;
pub mod sim;

pub use geom::Pose;
pub use network::{Components, Link, LinkId, Network, Node, NodeId, NodeKey};
pub use rail::{Railway, Train, TrainId, VehicleKind, VehiclePose};
pub use route::Route;
pub use sim::{
    CAR_FADE_TICKS, Car, CarId, CarPhase, HIGH_TIER_CAR_CAP, Home, MAX_LANE_OFFSET, ParkedCar,
    SimConfig, Simulation, SpotRef, TICK_HZ, TICK_SECONDS,
};
