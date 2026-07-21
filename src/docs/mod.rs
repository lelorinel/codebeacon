//! Sidecar documentation index and context anchors.
//!
//! Anchor syntax (copied from veld-anchor; independent of veld-lang):
//!
//! | Syntax             | Kind    |
//! |--------------------|---------|
//! | `path`             | Whole   |
//! | `path::Symbol`     | Symbol  |
//! | `path::## Heading` | Heading |
//! | `path#N-M`         | Lines   |

pub mod anchor;
pub mod brief;
pub mod index;

pub use anchor::{parse_reference, resolve};
pub use brief::build_update_brief;
pub use index::{
    clear_stale_for_docs_file, load_docs_index, mark_stale_for_code_path, query_docs, reindex_docs,
    write_docs_index, DocsIndex,
};
