//! Screen capture (ext-image-copy-capture) of an output: its screen's part of the drawn window.
//! Portals (xdg-desktop-portal-wlr) use this too, eg for OBS and browsers.

use super::SmithayState;
use crate::runner::AppletRunner;
use smithay::{
    output::{Output, WeakOutput},
    reexports::wayland_server::{DisplayHandle, protocol::wl_shm::Format},
    utils::Transform,
    wayland::{
        image_capture_source::{
            ImageCaptureSource, ImageCaptureSourceHandler, OutputCaptureSourceHandler,
            OutputCaptureSourceState,
        },
        image_copy_capture::{
            BufferConstraints, CaptureFailureReason, Frame, FrameRef, ImageCopyCaptureHandler,
            ImageCopyCaptureState, Session, SessionRef,
        },
        shm::with_buffer_contents_mut,
    },
};
use sonamu_common::sap::packets::StandardEvent;
use sonamu_ui::CapturedFrame;

#[derive(Debug)]
pub(super) struct CaptureState {
    output_source_state: OutputCaptureSourceState,
    copy_state: ImageCopyCaptureState,
    sessions: Vec<Session>,
    /// Waiting for the next draw, with their output
    frames: Vec<(Frame, WeakOutput)>,
}
impl CaptureState {
    pub(super) fn new(display_handle: &DisplayHandle) -> Self {
        Self {
            output_source_state: OutputCaptureSourceState::new::<AppletRunner>(display_handle),
            copy_state: ImageCopyCaptureState::new::<AppletRunner>(display_handle),
            sessions: Vec::new(),
            frames: Vec::new(),
        }
    }
}

/// Marks a session that got its first frame; later frames wait for damage, like the protocol asks
struct CapturedOnce;

impl SmithayState {
    /// The output's screen (window px), if it's still there
    fn output_screen(&self, output: &WeakOutput) -> Option<[u32; 4]> {
        let output = output.upgrade()?;
        let index = self.outputs.iter().position(|shown| *shown == output)?;
        self.screens.get(index).copied()
    }

    /// The output's, or `None` once it's gone
    fn capture_constraints(&self, output: &WeakOutput) -> Option<BufferConstraints> {
        let [_, _, width, height] = self.output_screen(output)?;
        Some(BufferConstraints {
            size: (width.cast_signed(), height.cast_signed()).into(),
            // both are B,G,R,A (or X) in bytes, like `CapturedFrame`
            shm: vec![Format::Argb8888, Format::Xrgb8888],
            dma: None,
        })
    }

    /// After the outputs' sizes changed
    pub(super) fn update_capture_constraints(&self) {
        for session in &self.capture.sessions {
            if let Some(constraints) = session
                .source()
                .user_data()
                .get::<WeakOutput>()
                .and_then(|output| self.capture_constraints(output))
            {
                session.update_constraints(constraints);
            }
        }
    }

    /// A frame is waiting for the next draw
    pub fn wants_capture(&self) -> bool {
        !self.capture.frames.is_empty()
    }

    /// Fills every waiting frame with the drawn frame (`None`: it couldn't be copied)
    pub fn frames_captured(&mut self, captured: Option<&CapturedFrame>) {
        let presented = self.clock.now();
        for (frame, output) in std::mem::take(&mut self.capture.frames) {
            let Some(screen) = self.output_screen(&output) else {
                frame.fail(CaptureFailureReason::Stopped);
                continue;
            };
            match captured.map(|captured| copy_into(&frame, captured, screen)) {
                Some(Ok(())) => frame.success(Transform::Normal, None, presented),
                Some(Err(reason)) => frame.fail(reason),
                None => frame.fail(CaptureFailureReason::Unknown),
            }
        }
    }
}

/// The screen's part of the window into the frame's shm buffer, at its top-left (cropped to it)
fn copy_into(
    frame: &Frame,
    captured: &CapturedFrame,
    [x, y, width, height]: [u32; 4],
) -> Result<(), CaptureFailureReason> {
    with_buffer_contents_mut(&frame.buffer(), |content_ptr, length, metadata| {
        // SAFETY: same as in `SmithayState::import_buffer`
        let pool = unsafe { std::slice::from_raw_parts_mut(content_ptr, length) };

        let x = x as usize * 4;
        let width = width.min(metadata.width.cast_unsigned()) as usize * 4;
        let height = height.min(metadata.height.cast_unsigned()) as usize;
        let row_len = captured.width as usize * 4;
        let rows = captured.bgra.chunks_exact(row_len).skip(y as usize);
        for (index, row) in rows.take(height).enumerate() {
            let start = (metadata.offset + metadata.stride * index as i32).cast_unsigned() as usize;
            let (Some(to), Some(from)) =
                (pool.get_mut(start..start + width), row.get(x..x + width))
            else {
                return Err(CaptureFailureReason::BufferConstraints);
            };
            to.copy_from_slice(from);
        }
        Ok(())
    })
    .map_err(|_| CaptureFailureReason::BufferConstraints)?
}

impl ImageCaptureSourceHandler for AppletRunner {}

impl OutputCaptureSourceHandler for AppletRunner {
    fn output_capture_source_state(&mut self) -> &mut OutputCaptureSourceState {
        &mut self.smithay_state.capture.output_source_state
    }

    fn output_source_created(&mut self, source: ImageCaptureSource, output: &Output) {
        source.user_data().insert_if_missing(|| output.downgrade());
    }
}

impl ImageCopyCaptureHandler for AppletRunner {
    fn image_copy_capture_state(&mut self) -> &mut ImageCopyCaptureState {
        &mut self.smithay_state.capture.copy_state
    }

    /// Only outputs can be captured
    fn capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        self.smithay_state
            .capture_constraints(source.user_data().get::<WeakOutput>()?)
    }

    fn new_session(&mut self, session: Session) {
        let sessions = &mut self.smithay_state.capture.sessions;
        sessions.push(session);
        if sessions.len() == 1 {
            self.root_client.send_event(StandardEvent::Capturing(true));
        }
    }

    fn frame(&mut self, session: &SessionRef, frame: Frame) {
        let Some(output) = session.source().user_data().get::<WeakOutput>().cloned() else {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        };
        self.smithay_state.capture.frames.push((frame, output));
        if session.user_data().get::<CapturedOnce>().is_none() {
            session.user_data().insert_if_missing(|| CapturedOnce);
            self.needs_redraw = true;
        }
    }

    fn frame_aborted(&mut self, frame: FrameRef) {
        self.smithay_state
            .capture
            .frames
            .retain(|(waiting, _)| *waiting != frame);
    }

    fn session_destroyed(&mut self, session: SessionRef) {
        let sessions = &mut self.smithay_state.capture.sessions;
        let had_sessions = !sessions.is_empty();
        sessions.retain(|kept| *kept != session);
        if had_sessions && sessions.is_empty() {
            self.root_client.send_event(StandardEvent::Capturing(false));
        }
    }
}
