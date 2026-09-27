//! Sequential column regions. Physical pages and columns have different lifetimes.

use super::{FINE_PER_TWIP, Page, PageSetup, Rect, Twips};
use crate::ColumnLayout;

#[derive(Clone)]
pub(super) struct FlowRegions {
    // Pending section geometry takes effect on the next physical page.
    setup: PageSetup,
    columns: ColumnLayout,
    // The current physical body remains authoritative for fit and oversize checks.
    body: Rect,
    areas: Vec<Rect>,
    column: usize,
    first_line: u32,
    region_base: usize,
    top_fine: i64,
}

impl FlowRegions {
    pub(super) fn new(setup: PageSetup, columns: ColumnLayout) -> Self {
        let mut flow = Self {
            setup,
            columns,
            body: setup.content_area(),
            areas: Vec::new(),
            column: 0,
            first_line: 0,
            region_base: 0,
            top_fine: i64::from(setup.content_area().y) * FINE_PER_TWIP,
        };
        flow.areas = flow.pending_areas();
        flow
    }

    fn pending_areas(&self) -> Vec<Rect> {
        let body = self.setup.content_area();
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

    pub(super) fn next_area(&self) -> Rect {
        self.areas
            .get(self.column + 1)
            .copied()
            .unwrap_or_else(|| self.pending_areas()[0])
    }

    pub(super) fn area_after_next(&self) -> Rect {
        if let Some(area) = self.areas.get(self.column + 2) {
            *area
        } else {
            let next_page = self.pending_areas();
            if self.column + 1 < self.areas.len() {
                next_page[0]
            } else {
                next_page.get(1).copied().unwrap_or(next_page[0])
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
            i64::from(self.setup.content_area().height) * FINE_PER_TWIP
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
        self.areas = self.pending_areas();
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
        let mut page = Page::new(self.setup.size, self.setup.content_area());
        page.columns = self.areas.clone();
        page
    }

    pub(super) fn reset_empty_page(&mut self, page: &mut Page, line_index: &mut u32) -> Rect {
        self.body = self.setup.content_area();
        self.areas = self.pending_areas();
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
            self.body = self.setup.content_area();
            self.areas = self.pending_areas();
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
