//! GFM §4.10 tables.
//!
//! Two structural invariants of a well-formed table are lifted into
//! the type system here:
//!
//! 1. **Column count agreement.** A constructed [`TableBlock`] cannot
//!    carry a row whose cell count differs from `align.len()`. The
//!    GFM §4.10 reconciliation — "If there are fewer cells in a row
//!    than there are headers, the missing cells are treated as empty.
//!    If there are more, the excess is ignored." — happens once, at
//!    IR build time, inside [`TableRow::from_raw`].
//! 2. **Non-empty alignment.** A table with zero columns is not
//!    representable; [`TableBlock::try_new`] rejects it. Pulldown-
//!    cmark never produces a `Tag::Table(aligns)` with `aligns.len()
//!    == 0` from valid input, so this is purely a defensive guard.
//!
//! Cells are stored as [`NodeId`] handles into the surrounding
//! [`crate::tree::Tree`] arena, in the same style as
//! [`crate::cm::block::list::ListItem`]. The inline content of each
//! cell already carries the correct
//! [`crate::cm::inline::escape_policy::EscapeScope::in_table_cell`]
//! decision because the [`crate::tree::TreeBuilder`] sets that scope
//! at `Tag::TableCell` start; the typed view inherits the escape
//! choices for free.

use std::cell::OnceCell;
use std::marker::PhantomData;

use unicode_width::UnicodeWidthStr;

use crate::tree::{Node, NodeId, TableAlign};

/// One cell of a row. Carries only the arena id of the underlying
/// `NodeKind::TableCell` node; the cell's inline body lives in the
/// arena as that node's children, with escape policy already applied.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct TableCell {
    cell_id: NodeId,
}

impl TableCell {
    pub(crate) fn new(cell_id: NodeId) -> Self {
        Self { cell_id }
    }

    pub(crate) fn cell_id(self) -> NodeId {
        self.cell_id
    }
}

/// One row of a table. Constructed only via [`TableRow::from_raw`],
/// which enforces `cells.len() == expected_columns` by truncate-then-
/// pad. After construction, the invariant cannot be violated because
/// `cells` is private.
#[derive(Clone, Debug)]
pub(crate) struct TableRow {
    row_id: NodeId,
    cells: Vec<TableCell>,
}

impl TableRow {
    /// Reconcile a raw row to the table's column count, per GFM §4.10:
    /// "If there are fewer cells in a row than there are headers, the
    /// missing cells are treated as empty. If there are more, the
    /// excess is ignored."
    ///
    /// Padding cells point back at `row_id` rather than allocating a
    /// synthetic arena node; emitters render an empty cell whenever
    /// [`TableRow::is_pad`] returns `true`. Truncated cells are
    /// dropped silently.
    pub(crate) fn from_raw(
        row_id: NodeId,
        raw_cells: Vec<TableCell>,
        expected_columns: usize,
    ) -> Self {
        let mut cells = raw_cells;
        cells.truncate(expected_columns);
        while cells.len() < expected_columns {
            cells.push(TableCell { cell_id: row_id });
        }
        Self { row_id, cells }
    }

    pub(crate) fn row_id(&self) -> NodeId {
        self.row_id
    }

    pub(crate) fn cells(&self) -> &[TableCell] {
        &self.cells
    }

    /// True iff `cell` is a synthetic pad introduced by GFM §4.10
    /// reconciliation.
    pub(crate) fn is_pad(&self, cell: TableCell) -> bool {
        cell.cell_id == self.row_id
    }
}

/// A GFM §4.10 table whose well-formedness is guaranteed by
/// construction: non-empty alignment vector, head and every body row
/// have exactly `align.len()` cells.
#[derive(Debug)]
pub(crate) struct TableBlock<'a> {
    align: Vec<TableAlign>,
    head: TableRow,
    body: Vec<TableRow>,
    column_widths: OnceCell<Vec<usize>>,
    _phantom: PhantomData<&'a ()>,
}

impl Clone for TableBlock<'_> {
    fn clone(&self) -> Self {
        Self {
            align: self.align.clone(),
            head: self.head.clone(),
            body: self.body.clone(),
            // Don't carry the cached widths across clones: the cache
            // is bound to the source and arena of the original
            // computation, and re-deriving it is cheap.
            column_widths: OnceCell::new(),
            _phantom: PhantomData,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TableError {
    /// `align` was empty. Pulldown does not produce this from valid
    /// input; the IR builder falls back to legacy emission.
    NoColumns,
    /// The head row's cell count does not match `align.len()`. The
    /// IR builder runs [`TableRow::from_raw`] before
    /// [`TableBlock::try_new`], so this only fires when the caller
    /// constructs rows by hand.
    HeadColumnCountMismatch { expected: usize, got: usize },
    /// A body row's cell count does not match `align.len()`. Same
    /// caveat as [`TableError::HeadColumnCountMismatch`].
    BodyColumnCountMismatch {
        row: usize,
        expected: usize,
        got: usize,
    },
}

impl TableBlock<'_> {
    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(columns = align.len(), body_rows = body.len())
    )]
    pub(crate) fn try_new(
        align: Vec<TableAlign>,
        head: TableRow,
        body: Vec<TableRow>,
    ) -> Result<Self, TableError> {
        if align.is_empty() {
            return Err(TableError::NoColumns);
        }
        let expected = align.len();
        if head.cells.len() != expected {
            return Err(TableError::HeadColumnCountMismatch {
                expected,
                got: head.cells.len(),
            });
        }
        for (i, row) in body.iter().enumerate() {
            if row.cells.len() != expected {
                return Err(TableError::BodyColumnCountMismatch {
                    row: i,
                    expected,
                    got: row.cells.len(),
                });
            }
        }
        Ok(Self {
            align,
            head,
            body,
            column_widths: OnceCell::new(),
            _phantom: PhantomData,
        })
    }

    pub(crate) fn align(&self) -> &[TableAlign] {
        &self.align
    }

    pub(crate) fn head(&self) -> &TableRow {
        &self.head
    }

    pub(crate) fn body(&self) -> &[TableRow] {
        &self.body
    }

    /// Per-column maximum display width (Unicode East-Asian-Width
    /// aware) of cells' source slices. Computed lazily on first call;
    /// subsequent calls return the cached vector.
    /// Emit a GFM §4.10 table: head row, alignment row, body rows.
    /// Cells are rendered via the inline pretty-printer, line breaks
    /// inside cells collapsed to spaces. Per-column width is sized to
    /// the widest cell content (and the alignment marker minimum);
    /// rows that would otherwise exceed [`Wrap::columns`] fall back to
    /// content-width-only padding.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(
        &self,
        ctx: &crate::format::pretty::PrettyCtx<'b>,
        _id: NodeId,
    ) -> crate::format::doc::Doc<'b> {
        use crate::format::doc::{Doc, RenderOptions, concat, hard_line, render, text};

        let rows: Vec<Vec<String>> = core::iter::once(&self.head)
            .chain(self.body.iter())
            .map(|row| {
                row.cells
                    .iter()
                    .map(|cell| {
                        if row.is_pad(*cell) {
                            String::new()
                        } else {
                            let inline =
                                crate::format::inline::pretty_inline_children(ctx, cell.cell_id);
                            let raw = render(&inline, &RenderOptions);
                            normalize_table_cell(&raw)
                        }
                    })
                    .collect()
            })
            .collect();

        let n_cols = self.align.len();
        let widths = compute_column_widths(&rows, &self.align, n_cols, ctx.opts.wrap());

        let mut parts: Vec<Doc<'b>> =
            Vec::with_capacity(rows.len().saturating_mul(2).saturating_add(1));
        if let Some(head) = rows.first() {
            parts.push(text(format_table_row(head, &widths)));
            parts.push(hard_line());
            parts.push(text(format_alignment_row(&self.align, &widths)));
            parts.push(hard_line());
        }
        for row in rows.iter().skip(1) {
            parts.push(text(format_table_row(row, &widths)));
            parts.push(hard_line());
        }
        concat(parts)
    }

    pub(crate) fn column_widths(&self, source: &str, arena: &[Node<'_>]) -> &[usize] {
        self.column_widths.get_or_init(|| {
            let mut widths = vec![0_usize; self.align.len()];
            let rows = core::iter::once(&self.head).chain(self.body.iter());
            for row in rows {
                for (col, cell) in row.cells.iter().enumerate() {
                    let w = if row.is_pad(*cell) {
                        0
                    } else {
                        cell_display_width(source, arena, cell.cell_id)
                    };
                    if let Some(slot) = widths.get_mut(col)
                        && w > *slot
                    {
                        *slot = w;
                    }
                }
            }
            widths
        })
    }
}

fn normalize_table_cell(s: &str) -> String {
    s.replace('\n', " ")
}

fn compute_column_widths(
    rows: &[Vec<String>],
    alignments: &[TableAlign],
    n_cols: usize,
    wrap: crate::config::Wrap,
) -> Vec<usize> {
    let mut widths = vec![0usize; n_cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            let w = UnicodeWidthStr::width(cell.as_str());
            if let Some(slot) = widths.get_mut(i)
                && w > *slot
            {
                *slot = w;
            }
        }
    }
    for (i, slot) in widths.iter_mut().enumerate() {
        let a = alignments.get(i).copied().unwrap_or(TableAlign::None);
        let min = alignment_min_width(a);
        if min > *slot {
            *slot = min;
        }
    }
    let row_width: usize = widths
        .iter()
        .map(|w| w.saturating_add(3))
        .sum::<usize>()
        .saturating_add(1);
    let target = wrap.columns() as usize;
    if row_width > target {
        let mut acc = vec![0usize; n_cols];
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                let w = UnicodeWidthStr::width(cell.as_str());
                if let Some(slot) = acc.get_mut(i)
                    && w > *slot
                {
                    *slot = w;
                }
            }
        }
        return acc;
    }
    widths
}

const fn alignment_min_width(a: TableAlign) -> usize {
    match a {
        TableAlign::None => 3,
        TableAlign::Left | TableAlign::Right => 4,
        TableAlign::Center => 5,
    }
}

fn format_table_row(cells: &[String], widths: &[usize]) -> String {
    let mut out = String::from("|");
    for (i, c) in cells.iter().enumerate() {
        let w = widths.get(i).copied().unwrap_or(0);
        let pad = w.saturating_sub(UnicodeWidthStr::width(c.as_str()));
        out.push(' ');
        out.push_str(c);
        for _ in 0..pad {
            out.push(' ');
        }
        out.push_str(" |");
    }
    out
}

fn format_alignment_row(alignments: &[TableAlign], widths: &[usize]) -> String {
    let mut out = String::from("|");
    for (i, &w) in widths.iter().enumerate() {
        let a = alignments.get(i).copied().unwrap_or(TableAlign::None);
        out.push(' ');
        match a {
            TableAlign::None => {
                for _ in 0..w {
                    out.push('-');
                }
            }
            TableAlign::Left => {
                out.push(':');
                for _ in 0..w.saturating_sub(1) {
                    out.push('-');
                }
            }
            TableAlign::Right => {
                for _ in 0..w.saturating_sub(1) {
                    out.push('-');
                }
                out.push(':');
            }
            TableAlign::Center => {
                out.push(':');
                for _ in 0..w.saturating_sub(2) {
                    out.push('-');
                }
                out.push(':');
            }
        }
        out.push(' ');
        out.push('|');
    }
    out
}

/// Display width of a cell from its arena source range.
fn cell_display_width(source: &str, arena: &[Node<'_>], cell_id: NodeId) -> usize {
    let Some(node) = arena.get(cell_id.idx()) else {
        return 0;
    };
    let Some(slice) = source.get(node.raw_range.clone()) else {
        return 0;
    };
    UnicodeWidthStr::width(slice)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::*;

    fn nid(i: u32) -> NodeId {
        NodeId::from_index(i)
    }

    #[test]
    fn try_new_rejects_empty_align() {
        let head = TableRow::from_raw(nid(1), vec![], 0);
        let err = TableBlock::try_new(vec![], head, vec![]).unwrap_err();
        assert_eq!(err, TableError::NoColumns);
    }

    #[test]
    fn try_new_rejects_head_mismatch() {
        // Hand-built row to defeat from_raw's reconciliation.
        let head = TableRow::from_raw(nid(1), vec![TableCell::new(nid(2))], 1);
        let err = TableBlock::try_new(vec![TableAlign::None, TableAlign::None], head, vec![])
            .unwrap_err();
        assert!(matches!(
            err,
            TableError::HeadColumnCountMismatch {
                expected: 2,
                got: 1
            }
        ));
    }

    #[test]
    fn try_new_rejects_body_mismatch() {
        let head = TableRow::from_raw(
            nid(1),
            vec![TableCell::new(nid(2)), TableCell::new(nid(3))],
            2,
        );
        let body_row = TableRow::from_raw(nid(4), vec![TableCell::new(nid(5))], 1);
        let err = TableBlock::try_new(
            vec![TableAlign::None, TableAlign::None],
            head,
            vec![body_row],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            TableError::BodyColumnCountMismatch {
                row: 0,
                expected: 2,
                got: 1
            }
        ));
    }

    #[test]
    fn from_raw_truncates() {
        let row = TableRow::from_raw(
            nid(1),
            vec![
                TableCell::new(nid(2)),
                TableCell::new(nid(3)),
                TableCell::new(nid(4)),
            ],
            2,
        );
        assert_eq!(row.cells().len(), 2);
        assert_eq!(row.cells()[0].cell_id(), nid(2));
        assert_eq!(row.cells()[1].cell_id(), nid(3));
        assert!(!row.is_pad(row.cells()[0]));
        assert!(!row.is_pad(row.cells()[1]));
    }

    #[test]
    fn from_raw_pads_with_synthetic_cells() {
        let row = TableRow::from_raw(nid(1), vec![TableCell::new(nid(2))], 3);
        assert_eq!(row.cells().len(), 3);
        assert!(!row.is_pad(row.cells()[0]));
        assert!(row.is_pad(row.cells()[1]));
        assert!(row.is_pad(row.cells()[2]));
    }

    #[test]
    fn from_raw_exact_passes_through() {
        let row = TableRow::from_raw(
            nid(1),
            vec![TableCell::new(nid(2)), TableCell::new(nid(3))],
            2,
        );
        assert_eq!(row.cells().len(), 2);
        assert!(!row.is_pad(row.cells()[0]));
        assert!(!row.is_pad(row.cells()[1]));
    }

    #[test]
    fn column_widths_cached() {
        let head = TableRow::from_raw(
            nid(1),
            vec![TableCell::new(nid(2)), TableCell::new(nid(3))],
            2,
        );
        let t = TableBlock::try_new(vec![TableAlign::None; 2], head, vec![]).unwrap();
        // Empty arena → all widths derive to 0; cache still
        // initialises. Asserting the cell behaves identically across
        // calls is the cache's contract (same Vec, same contents).
        let arena: Vec<Node<'_>> = Vec::new();
        let w1 = t.column_widths("", &arena);
        let w2 = t.column_widths("", &arena);
        assert!(std::ptr::eq(w1, w2));
        assert_eq!(w1, &[0_usize, 0]);
    }

    #[test]
    fn try_new_accepts_well_formed_table() {
        let head = TableRow::from_raw(
            nid(1),
            vec![TableCell::new(nid(2)), TableCell::new(nid(3))],
            2,
        );
        let body = vec![TableRow::from_raw(
            nid(4),
            vec![TableCell::new(nid(5)), TableCell::new(nid(6))],
            2,
        )];
        let t = TableBlock::try_new(vec![TableAlign::Left, TableAlign::Right], head, body).unwrap();
        assert_eq!(t.align().len(), 2);
        assert_eq!(t.head().cells().len(), 2);
        assert_eq!(t.body().len(), 1);
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn nid(i: u32) -> NodeId {
        NodeId::from_index(i)
    }

    fn aligns_of(n_cols: usize) -> Vec<TableAlign> {
        const POOL: [TableAlign; 4] = [
            TableAlign::None,
            TableAlign::Left,
            TableAlign::Center,
            TableAlign::Right,
        ];
        (0..n_cols).map(|i| POOL[i % 4]).collect()
    }

    fn make_row(row_id: NodeId, raw_len: usize, expected: usize) -> TableRow {
        let raw: Vec<TableCell> = (0..raw_len)
            .map(|i| TableCell::new(nid(row_id.idx() as u32 + 1 + i as u32)))
            .collect();
        TableRow::from_raw(row_id, raw, expected)
    }

    proptest! {
        /// Hand-matched triples — every row already has exactly
        /// `n_cols` cells — must construct a valid `TableBlock`.
        #[test]
        fn matched_triples_always_validate(
            n_cols in 1usize..=8,
            n_body in 0usize..=8,
        ) {
            let aligns = aligns_of(n_cols);
            let head = make_row(nid(1), n_cols, n_cols);
            let body: Vec<TableRow> = (0..n_body)
                .map(|i| make_row(nid(1000 + i as u32 * 16), n_cols, n_cols))
                .collect();
            let t = TableBlock::try_new(aligns, head, body)
                .expect("matched triple validates");
            prop_assert_eq!(t.align().len(), n_cols);
            prop_assert_eq!(t.head().cells().len(), n_cols);
            for row in t.body() {
                prop_assert_eq!(row.cells().len(), n_cols);
            }
        }

        /// Rows of arbitrary raw lengths, normalised by
        /// `TableRow::from_raw`, always yield a valid `TableBlock`.
        #[test]
        fn from_raw_normalises_for_try_new(
            n_cols in 1usize..=6,
            head_len in 0usize..=10,
            body_lens in proptest::collection::vec(0usize..=10, 0..=6),
        ) {
            let aligns = aligns_of(n_cols);
            let head = make_row(nid(1), head_len, n_cols);
            let body: Vec<TableRow> = body_lens.iter().enumerate()
                .map(|(ri, &bl)| make_row(nid(2000 + ri as u32 * 32), bl, n_cols))
                .collect();
            let t = TableBlock::try_new(aligns, head, body)
                .expect("from_raw normalises");
            prop_assert_eq!(t.head().cells().len(), n_cols);
            for row in t.body() {
                prop_assert_eq!(row.cells().len(), n_cols);
            }
        }
    }
}
