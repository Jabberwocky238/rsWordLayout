//! Shared normal formatting and strict replay of the final continuous-section band.

use super::*;
use std::collections::VecDeque;

#[derive(Clone)]
struct ReplayStart {
    para: usize,
    source_base: u32,
    cursor: LineCursor,
    top_fine: i64,
    regions: FlowRegions,
    page: Page,
    line_index: u32,
}

struct Flow {
    regions: FlowRegions,
    pages: Vec<Page>,
    page: Page,
    line_index: u32,
    cursor_fine: i64,
    required_bottom: i64,
    previous_bottom: i64,
    replay: Option<ReplayStart>,
    trial_bottom: Option<i64>,
    next_bottom: Option<i64>,
}

impl Flow {
    fn new(setup: PageSetup, columns: crate::ColumnLayout) -> Self {
        let mut regions = FlowRegions::new(setup, columns);
        let mut page = Page::new(setup.size, setup.content_area());
        let mut line_index = 0;
        regions.reset_empty_page(&mut page, &mut line_index);
        let top = regions.top_fine();
        Self {
            regions,
            pages: Vec::new(),
            page,
            line_index,
            cursor_fine: top,
            required_bottom: top,
            previous_bottom: top,
            replay: None,
            trial_bottom: None,
            next_bottom: None,
        }
    }

    fn trial(start: &ReplayStart, bottom: i64) -> Self {
        let top = start.regions.top_fine();
        Self {
            regions: start.regions.clone(),
            pages: Vec::new(),
            page: start.page.clone(),
            line_index: start.line_index,
            cursor_fine: start.top_fine,
            required_bottom: top,
            previous_bottom: top,
            replay: None,
            trial_bottom: Some(bottom),
            next_bottom: None,
        }
    }

    fn bottom(&self) -> i64 {
        self.trial_bottom
            .unwrap_or_else(|| fine(self.regions.area().bottom()))
    }

    fn used_bottom(&self) -> i64 {
        self.required_bottom
            .max(self.previous_bottom)
            .max(self.cursor_fine)
    }

    fn keep_capacity(&self, next: bool) -> i64 {
        if self.trial_bottom.is_some() {
            i64::MAX
        } else if next {
            self.regions.next_full_height_fine()
        } else {
            self.regions.full_height_fine()
        }
    }

    // Every height-dependent rejection contributes the next capacity event.
    // Walking these events does not assume that greedy reflow is monotone.
    fn consider(&mut self, required_bottom: i64) {
        if self
            .trial_bottom
            .is_some_and(|bottom| required_bottom > bottom)
        {
            self.next_bottom = Some(
                self.next_bottom
                    .map_or(required_bottom, |old| old.min(required_bottom)),
            );
        }
    }

    fn reset_band(&mut self) {
        let top = self.regions.top_fine();
        self.cursor_fine = top;
        self.required_bottom = top;
        self.previous_bottom = top;
        self.replay = None;
    }

    fn reset_empty_page(&mut self) {
        self.regions
            .reset_empty_page(&mut self.page, &mut self.line_index);
        self.reset_band();
    }

    fn advance(&mut self, force_page: bool) -> Result<(), ()> {
        let new_page = self.regions.ends_page(force_page);
        if new_page && self.trial_bottom.is_some() {
            return Err(());
        }
        self.previous_bottom = self.previous_bottom.max(self.cursor_fine);
        self.regions.advance(
            &mut self.pages,
            &mut self.page,
            &mut self.line_index,
            force_page,
        );
        if new_page {
            self.reset_band();
        } else {
            self.cursor_fine = self.regions.top_fine();
        }
        Ok(())
    }

    fn record_start(&mut self, para: usize, source_base: u32, line: &PendingLine) {
        if self.trial_bottom.is_none() && self.replay.is_none() {
            self.replay = Some(ReplayStart {
                para,
                source_base,
                cursor: LineCursor {
                    source: line.source_start,
                    first: line.is_first,
                },
                top_fine: self.cursor_fine,
                regions: self.regions.clone(),
                page: self.page.clone(),
                line_index: self.line_index,
            });
        }
    }
}

impl<M: FontMetrics> Engine<'_, M> {
    pub(super) fn layout_sections(
        &self,
        paras: &[Para],
        sections: &[crate::LayoutSection],
    ) -> Vec<Page> {
        let setup = sections.first().map_or(self.setup, |s| s.setup);
        let columns = sections
            .first()
            .map_or_else(crate::ColumnLayout::default, |s| s.columns.clone());
        let mut flow = Flow::new(setup, columns);
        let mut source = 0;
        let mut active_section: Option<usize> = None;
        for (index, para) in paras.iter().enumerate() {
            if let Some((si, section)) = sections
                .iter()
                .enumerate()
                .find(|(_, s)| s.para_range.contains(&index))
                && active_section != Some(si)
            {
                use crate::SectionStart;
                let previous = active_section.map(|si| &sections[si]);
                let same_page = previous.is_some_and(|previous| previous.setup == section.setup)
                    && section.kind == SectionStart::Continuous
                    && flow.page.size == section.setup.size
                    && flow.page.content_area == section.setup.content_area();
                if same_page && !flow.page.fragments.is_empty() {
                    self.balance_band(paras, sections, index, &mut flow);
                    let bottom = flow.used_bottom();
                    flow.regions.close_band(&mut flow.page, bottom);
                    flow.regions
                        .set_section(section.setup, section.columns.clone());
                    if bottom >= fine(flow.page.content_area.bottom()) {
                        flow.advance(true).expect("normal page flow");
                    } else {
                        flow.regions
                            .start_band(&mut flow.page, flow.line_index, bottom);
                        flow.reset_band();
                    }
                } else {
                    flow.regions
                        .set_section(section.setup, section.columns.clone());
                    let new_page = !matches!(
                        section.kind,
                        SectionStart::Continuous | SectionStart::NextColumn
                    );
                    if previous.is_some() && new_page && !flow.page.fragments.is_empty() {
                        flow.advance(true).expect("normal page flow");
                    } else if previous.is_some()
                        && section.kind == SectionStart::NextColumn
                        && !flow.regions.is_empty(flow.line_index)
                    {
                        flow.advance(false).expect("normal column flow");
                    }
                    if flow.page.fragments.is_empty() {
                        flow.reset_empty_page();
                        if previous.is_some() {
                            let physical_page = flow.pages.len() + 1;
                            let wrong_parity = match section.kind {
                                SectionStart::EvenPage => !physical_page.is_multiple_of(2),
                                SectionStart::OddPage => physical_page.is_multiple_of(2),
                                _ => false,
                            };
                            if wrong_parity {
                                flow.pages.push(flow.page.clone());
                            }
                        }
                    }
                }
                active_section = Some(si);
            }
            self.format_flow_paragraph(paras, sections, index, source, None, &mut flow)
                .expect("normal formatting always advances");
            source += para
                .runs
                .iter()
                .map(|run| utf16_len(&run.text))
                .sum::<u32>()
                + 1;
        }
        flow.pages.push(flow.page);
        flow.pages
    }

    fn balance_band(
        &self,
        paras: &[Para],
        sections: &[crate::LayoutSection],
        end: usize,
        flow: &mut Flow,
    ) {
        if flow.regions.count() < 2 {
            return;
        }
        let Some(start) = flow.replay.clone() else {
            return;
        };
        let upper = fine(flow.regions.area().bottom());
        let mut bottom = flow.regions.top_fine();
        while bottom <= upper {
            let mut trial = Flow::trial(&start, bottom);
            let mut source = start.source_base;
            let mut fits = true;
            for index in start.para..end {
                if index != start.para
                    && let Some(section) = sections.iter().find(|s| s.para_range.start == index)
                {
                    trial
                        .regions
                        .set_section(section.setup, section.columns.clone());
                    let force_page = !matches!(
                        section.kind,
                        crate::SectionStart::Continuous | crate::SectionStart::NextColumn
                    );
                    let next_column = section.kind == crate::SectionStart::NextColumn
                        && !trial.regions.is_empty(trial.line_index);
                    if (force_page || next_column) && trial.advance(force_page).is_err() {
                        fits = false;
                        break;
                    }
                }
                let resume = (index == start.para).then_some(start.cursor);
                if self
                    .format_flow_paragraph(
                        &paras[..end],
                        sections,
                        index,
                        source,
                        resume,
                        &mut trial,
                    )
                    .is_err()
                {
                    fits = false;
                    break;
                }
                source += paras[index]
                    .runs
                    .iter()
                    .map(|run| utf16_len(&run.text))
                    .sum::<u32>()
                    + 1;
            }
            if fits {
                flow.page = trial.page;
                flow.regions = trial.regions;
                flow.line_index = trial.line_index;
                flow.cursor_fine = trial.cursor_fine;
                flow.required_bottom = trial.required_bottom;
                flow.previous_bottom = trial.previous_bottom;
                return;
            }
            let Some(next) = trial.next_bottom else {
                return;
            };
            debug_assert!(next > bottom);
            bottom = next;
        }
        // A truly oversized group keeps normal formatting's progress fallback.
        // Its occupied extent still determines where the successor can begin.
    }

    fn flow_quota(
        &self,
        paras: &[Para],
        sections: &[crate::LayoutSection],
        index: usize,
        source: std::ops::Range<u32>,
        pending: &VecDeque<PendingLine>,
        flow: &mut Flow,
    ) -> usize {
        let area = flow.regions.area();
        let next_region = flow.regions.next_region();
        let keep_after = if Self::keep_link(paras, sections, index) {
            let current = lines_extent(pending.iter());
            self.keep_after_extent(
                paras,
                sections,
                index,
                area,
                (
                    flow.cursor_fine + current.advance_fine,
                    source.end,
                    next_region,
                ),
                flow.keep_capacity(false),
            )
        } else {
            None
        };
        let (count, next_bottom) = self.page_line_quota(
            &paras[index],
            pending,
            PageFit {
                area,
                next_region,
                top_fine: flow.cursor_fine,
                bottom_fine: flow.bottom(),
                source_base: source.start,
                keep_after,
            },
        );
        if let Some(next) = next_bottom {
            flow.consider(next);
        }
        if paras[index].keep_lines {
            let segment = pending
                .iter()
                .position(|line| line.flow_break.is_hard())
                .map_or(pending.len(), |i| i + 1);
            let whole = lines_extent(pending.iter().take(segment));
            if count < segment
                && (flow.trial_bottom.is_some()
                    || (flow.regions.is_partial()
                        && whole.required_fine <= flow.regions.full_height_fine()))
            {
                flow.consider(flow.cursor_fine + whole.required_fine);
                return 0;
            }
        }
        count
    }

    fn format_flow_paragraph(
        &self,
        paras: &[Para],
        sections: &[crate::LayoutSection],
        index: usize,
        source_base: u32,
        resume: Option<LineCursor>,
        flow: &mut Flow,
    ) -> Result<(), ()> {
        let para = &paras[index];
        let source_end = source_base
            + para
                .runs
                .iter()
                .map(|run| utf16_len(&run.text))
                .sum::<u32>()
            + 1;
        if resume.is_none() {
            if para.page_break_before && !flow.page.fragments.is_empty() {
                flow.advance(true)?;
            }
            flow.cursor_fine += fine(para.space_before);
        }
        let mut area = flow.regions.area();
        let start = resume.unwrap_or(LineCursor {
            source: source_base,
            first: true,
        });
        let mut lines =
            self.break_paragraph_at(para, area, flow.cursor_fine, source_base, start);
        let segment = lines
            .iter()
            .position(|line| line.flow_break.is_hard())
            .map_or(lines.len(), |i| i + 1);
        let block = lines_extent(lines.iter().take(segment));
        if para.keep_lines && start.first && flow.cursor_fine + block.required_fine > flow.bottom()
        {
            flow.consider(flow.cursor_fine + block.required_fine);
            let next_region = flow.regions.next_region();
            let next_lines =
                self.break_paragraph_at(para, next_region.area, next_region.top_fine, source_base, start);
            let next_segment = next_lines
                .iter()
                .position(|line| line.flow_break.is_hard())
                .map_or(next_lines.len(), |i| i + 1);
            let next = lines_extent(next_lines.iter().take(next_segment));
            if next.required_fine <= flow.regions.next_full_height_fine() {
                if !flow.regions.is_empty(flow.line_index) || flow.regions.is_partial() {
                    flow.advance(false)?;
                    area = flow.regions.area();
                    lines = next_lines;
                }
                if flow.trial_bottom.is_some()
                    && flow.cursor_fine + next.required_fine > flow.bottom()
                {
                    flow.consider(flow.cursor_fine + next.required_fine);
                    return Err(());
                }
            }
        }
        if para.keep_next
            && !flow.regions.is_empty(flow.line_index)
            && !lines.iter().any(|line| line.flow_break.is_hard())
        {
            let current = lines_extent(lines.iter());
            let after = self.keep_after_extent(
                paras,
                sections,
                index,
                area,
                (
                    flow.cursor_fine + current.advance_fine,
                    source_end,
                    flow.regions.next_region(),
                ),
                flow.keep_capacity(false),
            );
            if let Some(after) = after
                && flow.cursor_fine + current.then(after).required_fine > flow.bottom()
            {
                flow.consider(flow.cursor_fine + current.then(after).required_fine);
                let next_region = flow.regions.next_region();
                let next_lines =
                    self.break_paragraph_at(para, next_region.area, next_region.top_fine, source_base, start);
                let next_extent = lines_extent(next_lines.iter());
                let next_after = self.keep_after_extent(
                    paras,
                    sections,
                    index,
                    next_region.area,
                    (
                        next_region.top_fine + next_extent.advance_fine,
                        source_end,
                        flow.regions.region_after_next(),
                    ),
                    flow.keep_capacity(true),
                );
                let needed = next_after
                    .map_or(next_extent, |after| next_extent.then(after))
                    .required_fine;
                if needed <= flow.regions.next_full_height_fine() {
                    flow.advance(false)?;
                    lines = next_lines;
                }
            }
        }
        let mut pending: VecDeque<_> = lines.into();
        let mut remaining = self.flow_quota(
            paras,
            sections,
            index,
            source_base..source_end,
            &pending,
            flow,
        );
        while !pending.is_empty() {
            if remaining == 0
                && (!flow.regions.is_empty(flow.line_index) || flow.regions.is_partial())
            {
                flow.advance(false)?;
                area = flow.regions.area();
                let line = pending.front().expect("pending source line");
                pending = self
                    .break_paragraph_at(
                        para,
                        area,
                        flow.cursor_fine,
                        source_base,
                        LineCursor {
                            source: line.source_start,
                            first: line.is_first,
                        },
                    )
                    .into();
                remaining = self.flow_quota(
                    paras,
                    sections,
                    index,
                    source_base..source_end,
                    &pending,
                    flow,
                );
                // An empty partial band must not use the full-page oversized fallback.
                if remaining == 0 && flow.regions.is_partial() {
                    continue;
                }
            }
            if remaining == 0 && flow.trial_bottom.is_some() {
                return Err(());
            }
            remaining = remaining.max(1);
            let line = pending.pop_front().expect("remaining source line");
            flow.record_start(index, source_base, &line);
            self.place_line(
                &mut flow.page,
                &line,
                para,
                flow.cursor_fine,
                flow.line_index,
            );
            flow.page.line_columns.push(flow.regions.column());
            flow.required_bottom = flow
                .required_bottom
                .max(flow.cursor_fine + line.vertical.required_fine);
            flow.cursor_fine += line.vertical.advance_fine;
            flow.line_index += 1;
            remaining -= 1;
            if line.flow_break.is_hard() {
                flow.advance(line.flow_break == FlowBreak::Page)?;
                area = flow.regions.area();
                if let Some(line) = pending.front() {
                    pending = self
                        .break_paragraph_at(
                            para,
                            area,
                            flow.cursor_fine,
                            source_base,
                            LineCursor {
                                source: line.source_start,
                                first: line.is_first,
                            },
                        )
                        .into();
                }
                remaining = self.flow_quota(
                    paras,
                    sections,
                    index,
                    source_base..source_end,
                    &pending,
                    flow,
                );
            }
        }
        flow.cursor_fine += fine(para.space_after);
        Ok(())
    }
}
