//! Timing the scene pass on the graphics card with timestamp queries, without ever waiting for it.
//!
//! A pass is given two query slots (start and end of the pass). After the frame's commands are
//! recorded, the two timestamps are resolved into a buffer and copied to a small readback buffer.
//! A frame or more later (after the commands were submitted) the readback buffer is mapped
//! asynchronously; when the map completes the milliseconds are read and the slot is free again.
//! The window never blocks on any of it; a result arrives two or three frames late.
//!
//! Where the device cannot do timestamp queries (OpenGL, some drivers) there is no `Timer` and
//! callers fall back to frame intervals.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use egui_wgpu::wgpu;

/// How many passes can be in flight at once.
const SLOTS: usize = 6;
/// `resolve_query_set` writes at multiples of this many bytes.
const RESOLVE_STRIDE: u64 = 256;

/// What kind of pass was timed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The cheap picture drawn while things move.
    Moving,
    /// A band of rows of the sharp picture.
    Band,
    /// A whole sharp picture in one pass (the benchmark).
    Whole,
}

/// One finished measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimedPass {
    pub kind: Kind,
    pub ms: f32,
    /// Pixels the pass shaded.
    pub pixels: u64,
    /// Rows of the picture (bands).
    pub rows: u32,
    /// Linear size of the picture relative to the window (1 = every pixel).
    pub scale: f32,
}

#[derive(Clone, Copy)]
struct Pending {
    kind: Kind,
    pixels: u64,
    rows: u32,
    scale: f32,
    /// The frame in which the commands were recorded.
    frame: u64,
}

enum SlotState {
    Free,
    /// Resolved and copied; waiting for its commands to be submitted.
    Recorded(Pending),
    /// Mapping; `ready` is set by the map callback.
    Mapping(Pending, Arc<AtomicU8>),
}

struct Slot {
    read: wgpu::Buffer,
    state: SlotState,
}

pub struct Timer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: Vec<Slot>,
    /// Nanoseconds per timestamp tick.
    period: f32,
    frame: u64,
    done: Vec<TimedPass>,
}

/// A claimed slot: the query indices for the pass, to hand back to `finish`.
pub struct Claim {
    slot: usize,
    pending: Pending,
}

impl Timer {
    /// A timer, or None when the device cannot take timestamps in render passes.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Timer> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let period = queue.get_timestamp_period();
        if !period.is_finite() || period <= 0.0 {
            return None;
        }
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("meltcad timing"),
            ty: wgpu::QueryType::Timestamp,
            count: (SLOTS * 2) as u32,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("meltcad timing resolve"),
            size: RESOLVE_STRIDE * SLOTS as u64,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let slots = (0..SLOTS)
            .map(|_| Slot {
                read: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("meltcad timing read"),
                    size: 16,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                state: SlotState::Free,
            })
            .collect();
        Some(Timer { queries, resolve, slots, period, frame: 0, done: Vec::new() })
    }

    /// Start of a frame's work: collect what finished, and ask for the results of passes whose
    /// commands have been submitted (any frame before this one).
    pub fn begin_frame(&mut self, device: &wgpu::Device) {
        self.frame += 1;
        // (lets finished map requests call back; cheap when nothing is waiting)
        let _ = device.poll(wgpu::PollType::Poll);
        self.collect();
        for slot in &mut self.slots {
            if let SlotState::Recorded(p) = slot.state
                && p.frame < self.frame
            {
                let ready = Arc::new(AtomicU8::new(0));
                let flag = ready.clone();
                slot.read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                    flag.store(if r.is_ok() { 1 } else { 2 }, Ordering::Release);
                });
                slot.state = SlotState::Mapping(p, ready);
            }
        }
    }

    fn collect(&mut self) {
        for slot in &mut self.slots {
            let SlotState::Mapping(p, ready) = &slot.state else { continue };
            let state = ready.load(Ordering::Acquire);
            if state == 0 {
                continue;
            }
            let p = *p;
            if state == 2 {
                // (the map failed: nothing to read, nothing to unmap)
                slot.state = SlotState::Free;
                continue;
            }
            if let Ok(view) = slot.read.slice(..).get_mapped_range() {
                let mut ticks = [0u64; 2];
                for (i, t) in ticks.iter_mut().enumerate() {
                    let mut b = [0u8; 8];
                    if let Some(src) = view.get(i * 8..i * 8 + 8) {
                        b.copy_from_slice(src);
                    }
                    *t = u64::from_le_bytes(b);
                }
                drop(view);
                if ticks[1] >= ticks[0] && ticks[0] != 0 {
                    let ms = (ticks[1] - ticks[0]) as f64 * self.period as f64 / 1.0e6;
                    if ms.is_finite() {
                        self.done.push(TimedPass { kind: p.kind, ms: ms as f32, pixels: p.pixels, rows: p.rows, scale: p.scale });
                    }
                }
            }
            slot.read.unmap();
            slot.state = SlotState::Free;
        }
    }

    /// Results since the last call.
    pub fn take(&mut self) -> Vec<TimedPass> {
        std::mem::take(&mut self.done)
    }

    /// Reserve a slot for a pass. None when all slots are busy (that pass is simply not timed).
    pub fn claim(&mut self, kind: Kind, pixels: u64, rows: u32, scale: f32) -> Option<Claim> {
        let slot = self.slots.iter().position(|s| matches!(s.state, SlotState::Free))?;
        // (marked busy now so that nobody else takes it; `finish` makes it Recorded)
        let pending = Pending { kind, pixels, rows, scale, frame: self.frame };
        self.slots.get_mut(slot)?.state = SlotState::Recorded(Pending { frame: u64::MAX, ..pending });
        Some(Claim { slot, pending })
    }

    /// The pass descriptor's timestamp writes for a claim.
    pub fn writes(&self, claim: &Claim) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(claim.slot as u32 * 2),
            end_of_pass_write_index: Some(claim.slot as u32 * 2 + 1),
        }
    }

    /// After the pass: resolve its two timestamps and copy them where they can be mapped.
    pub fn finish(&mut self, encoder: &mut wgpu::CommandEncoder, claim: Claim) {
        let at = claim.slot as u64 * RESOLVE_STRIDE;
        let q = claim.slot as u32 * 2;
        let Some(slot) = self.slots.get_mut(claim.slot) else { return };
        encoder.resolve_query_set(&self.queries, q..q + 2, &self.resolve, at);
        encoder.copy_buffer_to_buffer(&self.resolve, at, &slot.read, 0, 16);
        slot.state = SlotState::Recorded(claim.pending);
    }

    /// Hand a claim back unused (the pass was not recorded).
    pub fn cancel(&mut self, claim: Claim) {
        if let Some(slot) = self.slots.get_mut(claim.slot) {
            slot.state = SlotState::Free;
        }
    }

    /// For offscreen use, where the caller submits and waits itself: after `queue.submit`, map
    /// every recorded pass, wait for the device, and return all the results.
    pub fn flush_blocking(&mut self, device: &wgpu::Device) -> Vec<TimedPass> {
        self.frame += 1;
        for slot in &mut self.slots {
            if let SlotState::Recorded(p) = slot.state
                && p.frame < self.frame
            {
                let ready = Arc::new(AtomicU8::new(0));
                let flag = ready.clone();
                slot.read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                    flag.store(if r.is_ok() { 1 } else { 2 }, Ordering::Release);
                });
                slot.state = SlotState::Mapping(p, ready);
            }
        }
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        self.collect();
        self.take()
    }
}
