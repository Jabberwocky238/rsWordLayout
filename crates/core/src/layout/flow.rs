//! Sequential column regions. Physical pages and columns have different lifetimes.

use super::{FINE_PER_TWIP, Page, PageSetup, Rect, Twips};
use crate::ColumnLayout;

#[derive(Clone, Copy)]
pub(super) struct FlowRegion {
    pub area: Rect,
    // A continuous-section band can begin between integer twips.
    pub top_fine: i64,
}

#[derive(Clone)]
pub(super) struct FlowRegions {
    // Pending section geometry takes effect on the next physical page.
    setup: PageSetup,
    columns: ColumnLayout,
    mirror_margins: bool,
    // Document physical ordinal, independent of section page-number labels.
    // This travels with a replay checkpoint even when its trial has no pages Vec.
    page_index: usize,
    // The current physical body remains authoritative for fit and oversize checks.
    body: Rect,
    areas: Vec<Rect>,
    column: usize,
    first_line: u32,
    region_base: usize,
    top_fine: i64,
}

impl FlowRegions {
    pub(super) fn new(setup: PageSetup, columns: ColumnLayout, mirror_margins: bool) -> Self {
        let mut flow = Self {
            setup,
            columns,
            mirror_margins,
            page_index: 0,
            body: setup.content_area(),
            areas: Vec::new(),
            column: 0,
            first_line: 0,
            region_base: 0,
            top_fine: i64::from(setup.content_area().y) * FINE_PER_TWIP,
        };
        flow.areas = flow.pending_areas(0);
        flow
    }

    fn body_at(&self, mut setup: PageSetup, page_index: usize) -> Rect {
        if self.mirror_margins && !page_index.is_multiple_of(2) {
            std::mem::swap(&mut setup.margins.left, &mut setup.margins.right);
        }
        setup.content_area()
    }

    pub(super) fn section_body(&self, setup: PageSetup) -> Rect {
        self.body_at(setup, self.page_index)
    }

    pub(super) fn physical_page_index(&self) -> usize {
        self.page_index
    }

    fn pending_areas(&self, page_index: usize) -> Vec<Rect> {
        let body = self.body_at(self.setup, page_index);
        // DOCX projection and overrides validate geometry. A manually edited
        // LayoutDocument can still be invalid; retain a usable body region.
        self.columns.areas(body).unwrap_or_else(|_| vec![body])
    }

    pub(super) fn set_section(&mut self, setup: PageSetup, columns: ColumnLayout) {
        self.setup = setup;
        self.columns = columns;
    }

    pub(super) fn area(&self) -> Rect {
        self.areas[self.column]
    }

    pub(super) fn next_region(&self) -> FlowRegion {
        self.region_ahead(1)
    }

    pub(super) fn region_after_next(&self) -> FlowRegion {
        self.region_ahead(2)
    }

    fn region_ahead(&self, offset: usize) -> FlowRegion {
        if let Some(&area) = self.areas.get(self.column + offset) {
            FlowRegion {
                area,
                top_fine: self.top_fine,
            }
        } else {
            let next_page = self.pending_areas(self.page_index + 1);
            let remaining = self.column + offset - self.areas.len();
            let pages_ahead = 1 + remaining / next_page.len();
            let index = remaining % next_page.len();
            // A two-region lookahead can span two physical pages in one-column
            // flow. Pending columns may also differ from the current page's.
            let next_page = if pages_ahead == 1 {
                next_page
            } else {
                self.pending_areas(self.page_index + pages_ahead)
            };
            let area = next_page[index];
            FlowRegion {
                area,
                top_fine: i64::from(area.y) * FINE_PER_TWIP,
            }
        }
    }

    pub(super) fn column(&self) -> usize {
        self.region_base + self.column
    }

    pub(super) fn count(&self) -> usize {
        self.areas.len()
    }

    pub(super) fn top_fine(&self) -> i64 {
        self.top_fine
    }

    pub(super) fn full_height_fine(&self) -> i64 {
        i64::from(self.body.height) * FINE_PER_TWIP
    }

    pub(super) fn next_full_height_fine(&self) -> i64 {
        if self.ends_page(false) {
            i64::from(self.body_at(self.setup, self.page_index + 1).height) * FINE_PER_TWIP
        } else {
            self.full_height_fine()
        }
    }

    pub(super) fn ends_page(&self, force_page: bool) -> bool {
        force_page || self.column + 1 == self.areas.len()
    }

    pub(super) fn is_partial(&self) -> bool {
        self.top_fine > i64::from(self.body.y) * FINE_PER_TWIP
    }

    pub(super) fn close_band(&self, page: &mut Page, bottom_fine: i64) {
        let bottom = (bottom_fine as f64 / FINE_PER_TWIP as f64).round() as Twips;
        for area in &mut page.columns[self.region_base..] {
            area.height = (bottom - area.y).max(0);
        }
    }

    pub(super) fn start_band(&mut self, page: &mut Page, line_index: u32, top_fine: i64) {
        self.areas = self.pending_areas(self.page_index);
        let top = (top_fine as f64 / FINE_PER_TWIP as f64).round() as Twips;
        for area in &mut self.areas {
            area.height = (area.bottom() - top).max(0);
            area.y = top;
        }
        self.top_fine = top_fine;
        self.column = 0;
        self.first_line = line_index;
        self.region_base = page.columns.len();
        page.columns.extend_from_slice(&self.areas);
    }

    pub(super) fn is_empty(&self, line_index: u32) -> bool {
        line_index == self.first_line
    }

    fn fresh_page(&self) -> Page {
        let mut page = Page::new(self.setup.size, self.body);
        page.columns = self.areas.clone();
        page
    }

    pub(super) fn reset_empty_page(&mut self, page: &mut Page, line_index: &mut u32) -> Rect {
        self.body = self.section_body(self.setup);
        self.areas = self.pending_areas(self.page_index);
        self.column = 0;
        self.first_line = 0;
        self.region_base = 0;
        self.top_fine = i64::from(self.area().y) * FINE_PER_TWIP;
        *line_index = 0;
        *page = self.fresh_page();
        self.area()
    }

    pub(super) fn advance(
        &mut self,
        pages: &mut Vec<Page>,
        page: &mut Page,
        line_index: &mut u32,
        force_page: bool,
    ) -> Rect {
        if self.ends_page(force_page) {
            self.page_index += 1;
            self.body = self.section_body(self.setup);
            self.areas = self.pending_areas(self.page_index);
            self.column = 0;
            self.region_base = 0;
            self.top_fine = i64::from(self.area().y) * FINE_PER_TWIP;
            pages.push(std::mem::replace(page, self.fresh_page()));
            *line_index = 0;
        } else {
            self.column += 1;
        }
        self.first_line = *line_index;
        self.area()
    }
}
