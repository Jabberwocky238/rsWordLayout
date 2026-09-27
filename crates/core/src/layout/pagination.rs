//! Shared normal formatting and strict replay of the final continuous-section band.

use super::*;
use std::collections::VecDeque;

#[derive(Clone, Copy)]
struct KeepContinuation {
    last_para: usize,
    relaxed: bool,
}

struct FlowQuota {
    count: usize,
    successor: Option<KeepPrefix>,
}

#[derive(Clone)]
struct ReplayStart {
    para: usize,
    source_base: u32,
    cursor: LineCursor,
    top_fine: i64,
    regions: FlowRegions,
    page: Page,
    line_index: u32,
    keep_continuation: Option<KeepContinuation>,
    keep_prefix: Option<KeepPrefix>,
}

struct Flow {
    regions: FlowRegions,
    pages: Vec<Page>,
    page: Page,
    line_index: u32,
    cursor_fine: i64,
    required_bottom: i64,
    previous_bottom: i64,
    // Already consumed after-space of the paragraph completed in this section
    // and region.
    // A document predecessor alone cannot establish adjacency after a flow break.
    paragraph_after: Option<Twips>,
    // Continuation survives automatic flow changes; an actual prefix promise
    // belongs only to the region where its predecessor was committed.
    keep_continuation: Option<KeepContinuation>,
    keep_prefix: Option<KeepPrefix>,
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
            paragraph_after: None,
            keep_continuation: None,
            keep_prefix: None,
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
            // The replay origin already includes the first paragraph's before
            // space. Its resumed formatter must not consume that space again.
            paragraph_after: None,
            keep_continuation: start.keep_continuation,
            keep_prefix: start.keep_prefix,
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
        if next {
            self.regions.next_full_height_fine()
        } else {
            self.regions.full_height_fine()
        }
    }

    fn keep_is_relaxed(&self) -> bool {
        self.keep_continuation.is_some_and(|keep| keep.relaxed)
    }

    fn select_keep(&mut self, reservation: KeepReservation, current: VerticalExtent) -> (VerticalExtent, KeepPrefix) {
        let relaxed = self.keep_is_relaxed()
            || current.then(reservation.preferred).required_fine > self.keep_capacity(false);
        self.keep_continuation = Some(KeepContinuation {
            last_para: self.keep_continuation.map_or(reservation.last_para, |keep| {
                keep.last_para.max(reservation.last_para)
            }),
            relaxed,
        });
        reservation.selected(relaxed)
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
        self.paragraph_after = None;
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
        self.paragraph_after = None;
        self.keep_prefix = None;
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
                keep_continuation: self.keep_continuation,
                keep_prefix: self.keep_prefix,
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
                flow.paragraph_after = trial.paragraph_after;
                flow.keep_continuation = trial.keep_continuation;
                flow.keep_prefix = trial.keep_prefix;
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
    ) -> FlowQuota {
        let area = flow.regions.area();
        let next_region = flow.regions.next_region();
        let current = lines_extent(pending.iter());
        let selected = if !pending.is_empty() && !pending.iter().any(|line| line.flow_break.is_hard()) {
            self.keep_reservation(
                paras,
                sections,
                index,
                area,
                (
                    flow.cursor_fine + current.advance_fine,
                    source.end,
                    next_region,
                ),
                KeepPolicy {
                    capacity: flow.keep_capacity(false),
                    immediate_only: flow.keep_is_relaxed(),
                },
            )
            .map(|reservation| flow.select_keep(reservation, current))
        } else {
            None
        };
        let segment = pending.iter().position(|line| line.flow_break.is_hard())
            .map_or(pending.len(), |i| i + 1);
        let whole = lines_extent(pending.iter().take(segment));
        let keep_lines_feasible = paras[index].keep_lines
            && whole.required_fine <= flow.keep_capacity(false);
        let quota = |keep_after| {
            let (count, next_bottom) = self.page_line_quota(&paras[index], pending, PageFit {
                area,
                next_region,
                top_fine: flow.cursor_fine,
                bottom_fine: flow.bottom(),
                source_base: source.start,
                keep_after,
            });
            if count < segment && keep_lines_feasible {
                let whole_bottom = flow.cursor_fine + whole.required_fine;
                (0, Some(next_bottom.map_or(whole_bottom, |next| next.max(whole_bottom))))
            } else {
                (count, next_bottom)
            }
        };
        let (mut count, mut next_bottom) = quota(selected.map(|(extent, _)| extent));
        let mut successor = selected.map(|(_, prefix)| prefix);
        let incoming = flow.keep_prefix.filter(|prefix| {
            prefix.source_start == source.start
                && pending.front().is_some_and(|line| line.source_start < prefix.source_end)
        });
        let incoming_unmet = incoming.is_some_and(|prefix| {
            count == 0 || pending[count - 1].source_end < prefix.source_end
        });
        let own_constraint_conflict = count == 0
            && (keep_lines_feasible || paras[index].widow_control)
            && whole.required_fine <= flow.keep_capacity(false)
            && selected.is_some_and(|(after, _)| {
                current.then(after).required_fine > flow.keep_capacity(false)
            });
        if incoming_unmet || own_constraint_conflict {
            // A later outgoing keep may conflict with an already accepted
            // prefix. Prefer the accepted prefix only if real fit and the
            // paragraph's widow/keepLines constraints still allow it. Without
            // an incoming promise, also preserve a feasible paragraph shape
            // when that shape plus the outgoing prefix exceeds the physical
            // body. A short balance trial alone must not create this conflict.
            let (without_keep, next) = quota(None);
            if without_keep > 0 && incoming.is_none_or(|prefix| {
                pending[without_keep - 1].source_end >= prefix.source_end
            }) {
                count = without_keep;
                next_bottom = next;
                successor = None;
            }
        }
        if let Some(next) = next_bottom {
            flow.consider(next);
        }
        FlowQuota {
            count,
            successor: if count == pending.len() && count > 0 { successor } else { None },
        }
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
        if flow.keep_continuation.is_some_and(|keep| index > keep.last_para) {
            flow.keep_continuation = None;
        }
        if flow.keep_prefix.is_some_and(|prefix| {
            prefix.source_start != source_base
                || resume.is_some_and(|cursor| cursor.source >= prefix.source_end)
        }) {
            flow.keep_prefix = None;
        }
        let source_end = source_base
            + para
                .runs
                .iter()
                .map(|run| utf16_len(&run.text))
                .sum::<u32>()
            + 1;
        if resume.is_none() {
            // Preserve section-boundary spacing, including continuous sections
            // whose geometry is deferred until a later physical page.
            if sections.iter().any(|s| s.para_range.start == index) {
                flow.paragraph_after = None;
            }
            if para.page_break_before && !flow.page.fragments.is_empty() {
                flow.advance(true)?;
            }
            flow.cursor_fine += flow.paragraph_after.take().map_or(
                fine(para.space_before),
                |after| paragraph_gap_fine(after, para.space_before) - fine(after),
            );
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
            && flow.keep_prefix.is_none()
        {
            let current = lines_extent(lines.iter());
            let after = self.keep_reservation(
                paras,
                sections,
                index,
                area,
                (
                    flow.cursor_fine + current.advance_fine,
                    source_end,
                    flow.regions.next_region(),
                ),
                KeepPolicy {
                    capacity: flow.keep_capacity(false),
                    immediate_only: flow.keep_is_relaxed(),
                },
            ).map(|reservation| flow.select_keep(reservation, current).0);
            if let Some(after) = after
                && flow.cursor_fine + current.then(after).required_fine > flow.bottom()
            {
                flow.consider(flow.cursor_fine + current.then(after).required_fine);
                let next_region = flow.regions.next_region();
                let next_lines =
                    self.break_paragraph_at(para, next_region.area, next_region.top_fine, source_base, start);
                let next_extent = lines_extent(next_lines.iter());
                let next_after = self.keep_reservation(
                    paras,
                    sections,
                    index,
                    next_region.area,
                    (
                        next_region.top_fine + next_extent.advance_fine,
                        source_end,
                        flow.regions.region_after_next(),
                    ),
                    KeepPolicy {
                        capacity: flow.keep_capacity(true),
                        immediate_only: flow.keep_is_relaxed(),
                    },
                );
                let needed = next_after
                    .map_or(next_extent, |reservation| {
                        let relaxed = flow.keep_is_relaxed()
                            || next_extent.then(reservation.preferred).required_fine > flow.keep_capacity(true);
                        next_extent.then(reservation.selected(relaxed).0)
                    })
                    .required_fine;
                if needed <= flow.regions.next_full_height_fine() {
                    flow.advance(false)?;
                    if let Some(reservation) = next_after {
                        flow.select_keep(reservation, next_extent);
                    }
                    lines = next_lines;
                }
            }
        }
        let mut pending: VecDeque<_> = lines.into();
        let mut quota = self.flow_quota(
            paras,
            sections,
            index,
            source_base..source_end,
            &pending,
            flow,
        );
        while !pending.is_empty() {
            if quota.count == 0
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
                quota = self.flow_quota(
                    paras,
                    sections,
                    index,
                    source_base..source_end,
                    &pending,
                    flow,
                );
                // An empty partial band must not use the full-page oversized fallback.
                if quota.count == 0 && flow.regions.is_partial() {
                    continue;
                }
            }
            if quota.count == 0 && flow.trial_bottom.is_some() {
                return Err(());
            }
            let accepted = quota.count > 0;
            quota.count = quota.count.max(1);
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
            quota.count -= 1;
            if flow.keep_prefix.is_some_and(|prefix| line.source_end >= prefix.source_end) {
                flow.keep_prefix = None;
            }
            if accepted && pending.is_empty() && !line.flow_break.is_hard() {
                flow.keep_prefix = quota.successor;
            }
            if line.flow_break.is_hard() {
                flow.keep_continuation = None;
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
                quota = self.flow_quota(
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
        // A final hard break may have opened an empty region. Its paragraph did
        // not finish there, so the following before-space cannot collapse here.
        flow.paragraph_after = (!flow.regions.is_empty(flow.line_index))
            .then_some(para.space_after);
        Ok(())
    }
}
