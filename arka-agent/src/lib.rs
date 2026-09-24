//! arka-agent library surface. The binary (`src/main.rs`) is a thin shell over
//! this; integration tests and the eval harness use these modules directly with
//! the mock backend + mock LLM, so everything is provable green with no GPU and
//! no running daemon.

pub mod agent;
pub mod approval;
pub mod audit;
pub mod backend;
pub mod cli;
pub mod config;
pub mod facts;
pub mod ollama;
pub mod sanitize;
pub mod schema;
pub mod scope;
pub mod tools;

#[cfg(test)]
mod write_tests;

#[cfg(test)]
mod eval;

