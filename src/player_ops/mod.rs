//! Authoritative player transactions shared by local and network adapters.

pub(crate) mod container;
pub(crate) mod craft;
pub(crate) mod equipment;
pub(crate) mod trade;

pub(crate) mod combat;
pub(crate) mod feeding;
pub(crate) mod nutrition;
pub(crate) mod terrain;

#[cfg(test)]
mod admission_tests;
#[cfg(test)]
mod container_tests;
#[cfg(test)]
mod trade_tests;
