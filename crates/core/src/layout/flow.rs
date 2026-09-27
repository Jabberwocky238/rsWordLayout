//! Sequential column regions. Physical pages and columns have different lifetimes.

use super::{Page, PageSetup, Rect};
use crate::ColumnLayout;

pub(super) struct FlowRegions {
    setup: PageSetup,
    columns: ColumnLayout,
    areas: Vec<Rect>,
    column: usize,
    first_line: u32,
}

impl FlowRegions {
    pub(super) fn new(setup: PageSetup, columns: ColumnLayout) -> Self {
        let mut flow = Self {
            setup,
            columns,
            areas: Vec::new(),
            column: 0,
            first_line: 0,
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
        self.column
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
        self.areas = self.pending_areas();
        self.column = 0;
        self.first_line = 0;
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
        if force_page || self.column + 1 == self.areas.len() {
            self.areas = self.pending_areas();
            self.column = 0;
            pages.push(std::mem::replace(page, self.fresh_page()));
            *line_index = 0;
        } else {
            self.column += 1;
        }
        self.first_line = *line_index;
        self.area()
    }
}
