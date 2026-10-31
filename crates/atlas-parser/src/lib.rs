//! TS/TSX import extraction and local import resolution.
//!
//! Source files are data here. They are parsed with Tree-sitter and never
//! evaluated, and syntax errors produce a partial result rather than a failure.

mod extract;
mod resolve;
mod tsconfig;

pub use extract::parse_source;
pub use resolve::{Resolution, Resolver};
pub use tsconfig::{ParsedTsconfig, parse_tsconfig};
