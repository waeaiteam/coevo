//! coevo-store: SQLite persistence layer with sqlx.
#![allow(
    clippy::redundant_closure,
    clippy::too_many_arguments,
    clippy::unnecessary_lazy_evaluations
)]

pub(crate) mod enum_db;
pub mod company_workspace;
pub mod migrate;
pub mod models;
pub mod pool;
pub mod repos;
pub mod repos_opc;
pub mod seed;
