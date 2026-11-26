//! Pagination helpers for MiraclePay v0.0.1.
//!
//! Clean-room, original implementation. Provides a small, reusable
//! pagination primitive for list endpoints: parse a page request,
//! slice an in-memory collection into a window, and describe that
//! window with metadata the client can render page controls from.

use serde::{Deserialize, Serialize};

/// Default page number used when the caller does not supply one.
pub const DEFAULT_PAGE: u32 = 1;

/// Default page size used when the caller does not supply one.
pub const DEFAULT_PER_PAGE: u32 = 20;

/// Maximum page size a caller may request. Larger requests are
/// clamped to this value so a single response stays bounded.
pub const MAX_PER_PAGE: u32 = 100;

/// Query parameters for a paginated list request.
///
/// Both fields are optional; absent values fall back to
/// [`DEFAULT_PAGE`] and [`DEFAULT_PER_PAGE`] respectively. Raw
/// values are stored as supplied so deserialization never fails on
/// out-of-range input — normalization happens in the accessors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct PageQuery {
    /// 1-based page number, if supplied.
    pub page: Option<u32>,
    /// Number of items per page, if supplied.
    pub per_page: Option<u32>,
}

impl PageQuery {
    /// Create a query from optional raw values.
    pub fn new(page: Option<u32>, per_page: Option<u32>) -> Self {
        Self { page, per_page }
    }

    /// Effective 1-based page number.
    ///
    /// Defaults to [`DEFAULT_PAGE`] and never returns `0`: a
    /// requested page of `0` is treated as the first page.
    pub fn page(&self) -> u32 {
        self.page.unwrap_or(DEFAULT_PAGE).max(1)
    }

    /// Effective page size, clamped to `1..=MAX_PER_PAGE`.
    ///
    /// Defaults to [`DEFAULT_PER_PAGE`] when not supplied.
    pub fn per_page(&self) -> u32 {
        self.per_page
            .unwrap_or(DEFAULT_PER_PAGE)
            .clamp(1, MAX_PER_PAGE)
    }

    /// Zero-based index of the first item on the effective page.
    ///
    /// Computed from the normalized [`Self::page`] and
    /// [`Self::per_page`], so it is consistent with the window
    /// [`paginate`] returns.
    pub fn offset(&self) -> usize {
        // page() >= 1 and per_page() >= 1, so this cannot underflow.
        // Both are u32; the product fits comfortably in a 64-bit usize.
        (self.page() as usize - 1) * self.per_page() as usize
    }
}

/// Metadata describing one page of a larger collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PageMeta {
    /// Effective 1-based page number that was returned.
    pub page: u32,
    /// Effective page size used for the window.
    pub per_page: u32,
    /// Total number of items in the full collection.
    pub total: u64,
    /// Total number of pages at the effective page size.
    ///
    /// This is `ceil(total / per_page)`, and `0` when the
    /// collection is empty.
    pub total_pages: u32,
}

impl PageMeta {
    /// Build metadata for `total` items under the given query.
    pub fn new(total: u64, query: &PageQuery) -> Self {
        let per_page = query.per_page();
        let total_pages = total_pages_for(total, per_page);
        Self {
            page: query.page(),
            per_page,
            total,
            total_pages,
        }
    }
}

/// Number of pages needed for `total` items at `per_page` per page.
///
/// Returns `0` for an empty collection rather than `1`, so clients
/// can distinguish "no results" from "one page of results".
fn total_pages_for(total: u64, per_page: u32) -> u32 {
    if total == 0 || per_page == 0 {
        return 0;
    }
    let per_page = u64::from(per_page);
    // Ceiling division without overflow: (total + per_page - 1) / per_page
    // rewritten using the remainder.
    let pages = total / per_page + u64::from(!total.is_multiple_of(per_page));
    // A collection addressable through this API cannot realistically
    // exceed u32::MAX pages; saturate instead of wrapping if it did.
    u32::try_from(pages).unwrap_or(u32::MAX)
}

/// Slice `items` down to the window requested by `query`.
///
/// Returns the cloned window together with its [`PageMeta`]. A page
/// beyond the end of the collection yields an empty window, while
/// the metadata still reports the true totals.
pub fn paginate<T: Clone>(items: &[T], query: &PageQuery) -> (Vec<T>, PageMeta) {
    let meta = PageMeta::new(items.len() as u64, query);
    let start = query.offset();
    if start >= items.len() {
        return (Vec::new(), meta);
    }
    let end = (start + query.per_page() as usize).min(items.len());
    (items[start..end].to_vec(), meta)
}

/// Apply optional `page`/`per_page` query parameters to a list
/// (Wave 11). When neither parameter is supplied the list is
/// returned unchanged, so endpoints keep their legacy behaviour
/// (including any cap they apply themselves) until a client opts
/// into paging. When either is supplied, the [`paginate`] window
/// for the effective [`PageQuery`] is returned.
pub fn apply_page<T: Clone>(items: Vec<T>, page: Option<u32>, per_page: Option<u32>) -> Vec<T> {
    if page.is_none() && per_page.is_none() {
        return items;
    }
    paginate(&items, &PageQuery::new(page, per_page)).0
}

/// A paginated list response: one page of data plus its metadata.
///
/// Generic over the item type so any list endpoint can return a
/// uniformly shaped body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PaginatedResponse<T> {
    /// The items on the requested page.
    pub data: Vec<T>,
    /// Metadata describing the page within the full collection.
    pub meta: PageMeta,
}

impl<T: Clone> PaginatedResponse<T> {
    /// Paginate `items` according to `query` and wrap the result.
    pub fn new(items: &[T], query: &PageQuery) -> Self {
        let (data, meta) = paginate(items, query);
        Self { data, meta }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(n: usize) -> Vec<u32> {
        (0..n as u32).collect()
    }

    #[test]
    fn defaults_are_first_page_of_twenty() {
        let query = PageQuery::default();
        assert_eq!(query.page(), 1);
        assert_eq!(query.per_page(), 20);
        assert_eq!(query.offset(), 0);
    }

    #[test]
    fn explicit_none_fields_also_use_defaults() {
        let query = PageQuery::new(None, None);
        assert_eq!(query.page(), DEFAULT_PAGE);
        assert_eq!(query.per_page(), DEFAULT_PER_PAGE);
    }

    #[test]
    fn per_page_above_max_is_clamped() {
        let query = PageQuery::new(Some(1), Some(500));
        assert_eq!(query.per_page(), 100);
        assert_eq!(query.per_page(), MAX_PER_PAGE);
    }

    #[test]
    fn per_page_zero_is_clamped_up_to_one() {
        let query = PageQuery::new(Some(1), Some(0));
        assert_eq!(query.per_page(), 1);
    }

    #[test]
    fn page_zero_is_treated_as_first_page() {
        let query = PageQuery::new(Some(0), Some(20));
        assert_eq!(query.page(), 1);
        assert_eq!(query.offset(), 0);
    }

    #[test]
    fn offset_advances_by_full_pages() {
        let query = PageQuery::new(Some(3), Some(20));
        assert_eq!(query.offset(), 40);
    }

    #[test]
    fn last_partial_page_windows_correctly() {
        let all = items(95);
        let query = PageQuery::new(Some(5), Some(20));
        let (window, meta) = paginate(&all, &query);

        assert_eq!(window.len(), 15);
        assert_eq!(window.first(), Some(&80));
        assert_eq!(window.last(), Some(&94));
        assert_eq!(meta.page, 5);
        assert_eq!(meta.per_page, 20);
        assert_eq!(meta.total, 95);
        assert_eq!(meta.total_pages, 5);
    }

    #[test]
    fn first_page_windows_from_the_start() {
        let all = items(95);
        let query = PageQuery::default();
        let (window, meta) = paginate(&all, &query);

        assert_eq!(window.len(), 20);
        assert_eq!(window.first(), Some(&0));
        assert_eq!(window.last(), Some(&19));
        assert_eq!(meta.total_pages, 5);
    }

    #[test]
    fn exact_multiple_has_no_partial_page() {
        let all = items(40);
        let query = PageQuery::new(Some(2), Some(20));
        let (window, meta) = paginate(&all, &query);

        assert_eq!(window.len(), 20);
        assert_eq!(meta.total_pages, 2);
    }

    #[test]
    fn page_beyond_end_is_empty_but_totals_hold() {
        let all = items(10);
        let query = PageQuery::new(Some(9), Some(20));
        let (window, meta) = paginate(&all, &query);

        assert!(window.is_empty());
        assert_eq!(meta.total, 10);
        assert_eq!(meta.total_pages, 1);
    }

    #[test]
    fn empty_list_has_zero_total_pages() {
        let all: Vec<u32> = Vec::new();
        let query = PageQuery::default();
        let (window, meta) = paginate(&all, &query);

        assert!(window.is_empty());
        assert_eq!(meta.total, 0);
        assert_eq!(meta.total_pages, 0);
        assert_eq!(meta.page, 1);
        assert_eq!(meta.per_page, 20);
    }

    #[test]
    fn paginated_response_wraps_window_and_meta() {
        let all = items(95);
        let query = PageQuery::new(Some(5), Some(20));
        let response = PaginatedResponse::new(&all, &query);

        assert_eq!(response.data.len(), 15);
        assert_eq!(response.meta.total, 95);
        assert_eq!(response.meta.total_pages, 5);
    }

    #[test]
    fn paginated_response_for_empty_list() {
        let all: Vec<String> = Vec::new();
        let response = PaginatedResponse::new(&all, &PageQuery::default());

        assert!(response.data.is_empty());
        assert_eq!(response.meta.total, 0);
        assert_eq!(response.meta.total_pages, 0);
    }
}

#[cfg(test)]
mod apply_page_tests {
    use super::*;

    #[test]
    fn no_params_returns_list_unchanged() {
        let items = vec![1, 2, 3, 4, 5];
        assert_eq!(apply_page(items, None, None), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn page_and_per_page_window_the_list() {
        let items: Vec<i32> = (1..=25).collect();
        assert_eq!(
            apply_page(items.clone(), Some(2), Some(10)),
            (11..=20).collect::<Vec<_>>()
        );
        assert_eq!(
            apply_page(items.clone(), Some(3), Some(10)),
            (21..=25).collect::<Vec<_>>()
        );
        assert!(apply_page(items, Some(9), Some(10)).is_empty());
    }

    #[test]
    fn per_page_alone_starts_at_first_page() {
        let items: Vec<i32> = (1..=5).collect();
        assert_eq!(apply_page(items, None, Some(2)), vec![1, 2]);
    }
}
