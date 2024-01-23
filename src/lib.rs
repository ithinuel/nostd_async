#![no_std]

//!
//! # Example
//!
//! ```
//! use nostd_async::{Runtime, Task};
//! let result = Runtime::new()
//!     .spawn(&mut Task::new(async {
//!         println!("Hello World");
//!         42
//!     })).join();
//!
//! assert_eq!(result, 42);
//! ```
//! See more examples in the [examples directory](https://github.com/sammhicks/nostd_async/tree/master/examples)

mod linked_list;
pub mod sync;
mod task;

pub use task::{JoinHandle, Runtime, Task};
