//! Screen capture (ext-image-copy-capture) of the one output, ie the whole Sonamu window.
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
    /// Waiting for the next draw
    frames: Vec<Frame>,
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
    /// The output's, or `None` before it has a size
    fn capture_constraints(&self) -> Option<BufferConstraints> {
        let size = self.output.current_mode()?.size;
        Some(BufferConstraints {
            size: (size.w, size.h).into(),
            // both are B,G,R,A (or X) in bytes, like `CapturedFrame`
            shm: vec![Format::Argb8888, Format::Xrgb8888],
            dma: None,
        })
    }

    /// After the output's size changed
    pub(super) fn update_capture_constraints(&self) {
        let Some(constraints) = self.capture_constraints() else {
            return;
        };
        for session in &self.capture.sessions {
            session.update_constraints(constraints.clone());
        }
    }

    /// A frame is waiting for the next draw
    pub fn wants_capture(&self) -> bool {
        !self.capture.frames.is_empty()
    }

    /// Fills every waiting frame with the drawn frame (`None`: it couldn't be copied)
    pub fn frames_captured(&mut self, captured: Option<&CapturedFrame>) {
        let presented = self.clock.now();
        for frame in self.capture.frames.drain(..) {
            match captured.map(|captured| copy_into(&frame, captured)) {
                Some(Ok(())) => frame.success(Transform::Normal, None, presented),
                Some(Err(reason)) => frame.fail(reason),
                None => frame.fail(CaptureFailureReason::Unknown),
            }
        }
    }
}

/// Into the frame's shm buffer, at its top-left (cropped to it)
fn copy_into(frame: &Frame, captured: &CapturedFrame) -> Result<(), CaptureFailureReason> {
    with_buffer_contents_mut(&frame.buffer(), |content_ptr, length, metadata| {
        // SAFETY: same as in `SmithayState::import_buffer`
        let pool = unsafe { std::slice::from_raw_parts_mut(content_ptr, length) };

        // only the output's part: with several screens, the main one, at the top left
        let width = captured.width.min(metadata.width.cast_unsigned()) as usize;
        let height = captured.height.min(metadata.height.cast_unsigned()) as usize;
        let row_len = captured.width as usize * 4;
        for (y, row) in captured.bgra.chunks_exact(row_len).take(height).enumerate() {
            let start = (metadata.offset + metadata.stride * y as i32).cast_unsigned() as usize;
            pool.get_mut(start..start + width * 4)
                .ok_or(CaptureFailureReason::BufferConstraints)?
                .copy_from_slice(&row[..width * 4]);
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

    /// Only outputs can be captured, and there is only the one
    fn capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        source.user_data().get::<WeakOutput>()?;
        self.smithay_state.capture_constraints()
    }

    fn new_session(&mut self, session: Session) {
        let sessions = &mut self.smithay_state.capture.sessions;
        sessions.push(session);
        if sessions.len() == 1 {
            self.root_client.send_event(StandardEvent::Capturing(true));
        }
    }

    fn frame(&mut self, session: &SessionRef, frame: Frame) {
        self.smithay_state.capture.frames.push(frame);
        if session.user_data().get::<CapturedOnce>().is_none() {
            session.user_data().insert_if_missing(|| CapturedOnce);
            self.needs_redraw = true;
        }
    }

    fn frame_aborted(&mut self, frame: FrameRef) {
        self.smithay_state
            .capture
            .frames
            .retain(|waiting| *waiting != frame);
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
