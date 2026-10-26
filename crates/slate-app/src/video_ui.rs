//! Import video: split a video into frames for a storyboard.
//!
//! The dialog reads the video, takes a frame count and a size, then runs one
//! cancellable background job. The frames land in a folder beside the video,
//! which opens as an ordinary folder collection when the job finishes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use gpui::{
    AnyElement, Context, ElementId, PathPromptOptions, SharedString, div, prelude::*, px, relative,
};
use slate_core::video::{self, ExtractOutcome, ExtractRequest, FrameSize, VideoInfo};

use crate::app::ContactSheet;
use crate::assets;
use crate::theme;
use crate::widgets::{
    button, dialog_footer, dialog_header, dialog_panel, disabled_button, icon_button, label,
    mini_button, modal, mono_text, primary_button, segment_button, ui_text,
};

/// The frame sizes the dialog offers.
const SIZES: [(FrameSize, &str); 3] = [
    (FrameSize::Original, "Full size"),
    (FrameSize::LongEdge(1920), "1920"),
    (FrameSize::LongEdge(1280), "1280"),
];

#[derive(Debug, Clone)]
pub enum VideoProbe {
    Reading,
    Ready(VideoInfo),
    Failed(String),
}

/// A running extraction. The background job reads `cancel` and counts
/// written frames in `done`; the dialog only reads them.
#[derive(Debug, Clone)]
pub struct VideoJob {
    cancel: Arc<AtomicBool>,
    done: Arc<AtomicUsize>,
    total: u32,
}

#[derive(Debug, Clone)]
pub struct VideoImport {
    video: PathBuf,
    folder: PathBuf,
    probe: VideoProbe,
    count: u32,
    size: FrameSize,
    job: Option<VideoJob>,
    error: Option<String>,
}

/// `1:02.4`, or `1:02:03` for an hour or more.
fn format_length(length: Duration) -> String {
    let tenths = length.as_millis() / 100;
    let (hours, minutes, seconds, tenth) = (
        tenths / 36_000,
        tenths / 600 % 60,
        tenths / 10 % 60,
        tenths % 10,
    );
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}.{tenth}")
    }
}

impl ContactSheet {
    pub(crate) fn import_video_prompt(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Ok(receiver) = cx.update(|app| {
                app.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Import video".into()),
                })
            }) else {
                return;
            };
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.open_video_import(path, cx));
        })
        .detach();
    }

    /// Show the dialog for `video` and read its length and size in the
    /// background.
    pub(crate) fn open_video_import(&mut self, video: PathBuf, cx: &mut Context<Self>) {
        if self
            .video_import
            .as_ref()
            .is_some_and(|import| import.job.is_some())
        {
            self.set_status("Frames are already being extracted");
            cx.notify();
            return;
        }
        self.video_import = Some(VideoImport {
            folder: video::frames_folder(&video),
            video: video.clone(),
            probe: VideoProbe::Reading,
            count: video::DEFAULT_FRAMES,
            size: FrameSize::Original,
            job: None,
            error: None,
        });
        let frames = self.video_frames_field.clone();
        cx.defer(move |cx| {
            frames.update(cx, |field, cx| {
                field.set_value(video::DEFAULT_FRAMES.to_string(), cx);
            });
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let task_video = video.clone();
            let probe = cx
                .background_spawn(async move { video::probe(&task_video) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(import) = this
                    .video_import
                    .as_mut()
                    .filter(|import| import.video == video)
                else {
                    return;
                };
                import.probe = match probe {
                    Ok(info) => VideoProbe::Ready(info),
                    Err(error) => VideoProbe::Failed(format!("{error:#}")),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// From the frames field, which passes its value so the field is never
    /// read from inside its own change handler.
    pub(crate) fn set_video_frames_text(&mut self, value: &str, cx: &mut Context<Self>) {
        if let (Some(import), Ok(count)) = (self.video_import.as_mut(), value.trim().parse::<u32>())
        {
            import.count = count.clamp(video::MIN_FRAMES, video::MAX_FRAMES);
            cx.notify();
        }
    }

    fn nudge_video_frames(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(import) = self.video_import.as_mut() else {
            return;
        };
        import.count = import
            .count
            .saturating_add_signed(delta)
            .clamp(video::MIN_FRAMES, video::MAX_FRAMES);
        let count = import.count;
        self.video_frames_field
            .update(cx, |field, cx| field.set_value(count.to_string(), cx));
        cx.notify();
    }

    fn start_video_extract(&mut self, cx: &mut Context<Self>) {
        let Some(import) = self.video_import.as_ref() else {
            return;
        };
        if import.job.is_some() || !matches!(import.probe, VideoProbe::Ready(_)) {
            return;
        }
        // The frames open as the collection when the job ends, so make sure
        // the current one can be left first.
        if !self.can_switch_collection(cx) {
            return;
        }
        let Some(import) = self.video_import.as_mut() else {
            return;
        };
        let request = ExtractRequest {
            video: import.video.clone(),
            count: import.count,
            size: import.size,
            folder: import.folder.clone(),
        };
        let job = VideoJob {
            cancel: Arc::new(AtomicBool::new(false)),
            done: Arc::new(AtomicUsize::new(0)),
            total: import.count,
        };
        import.job = Some(job.clone());
        import.error = None;
        let folder = request.folder.clone();
        self.log(format!("Video frames / {} requested", request.count));
        cx.notify();

        // Repaint progress while the job runs; the job itself never touches
        // the UI.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let running = this
                    .update(cx, |this, cx| {
                        cx.notify();
                        this.video_import
                            .as_ref()
                            .is_some_and(|import| import.job.is_some())
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();

        let (cancel, done) = (Arc::clone(&job.cancel), Arc::clone(&job.done));
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move { video::extract_frames(&request, &cancel, &done) })
                .await;
            let _ = this.update(cx, |this, cx| {
                // Ignore a job the dialog no longer shows.
                let Some(import) = this.video_import.as_mut().filter(|import| {
                    import
                        .job
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(&current.cancel, &job.cancel))
                }) else {
                    return;
                };
                import.job = None;
                match outcome {
                    Ok(ExtractOutcome::Finished(frames)) => {
                        this.video_import = None;
                        this.log(format!("Video frames / {} written", frames.len()));
                        this.open_project(&folder, cx);
                        this.set_status(format!("Extracted {} frames", frames.len()));
                    }
                    Ok(ExtractOutcome::Cancelled) => {
                        this.set_status("Frame extraction cancelled");
                    }
                    Err(error) => import.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Escape and the close button: stop a running job, or close the dialog.
    pub(crate) fn dismiss_video_import(&mut self, cx: &mut Context<Self>) {
        match self
            .video_import
            .as_ref()
            .and_then(|import| import.job.as_ref())
        {
            Some(job) => job.cancel.store(true, Ordering::Relaxed),
            None => self.video_import = None,
        }
        cx.notify();
    }

    pub(crate) fn render_video_import(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let import = self.video_import.clone()?;
        let name = import
            .video
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let summary: AnyElement = match &import.probe {
            VideoProbe::Reading => mono_text(theme::TEXT_META, theme::faint())
                .child("Reading…")
                .into_any_element(),
            VideoProbe::Ready(info) => {
                let (width, height) = info.display_size();
                let rate = info
                    .frame_rate
                    .map(|rate| format!(" / {rate:.2} fps"))
                    .unwrap_or_default();
                mono_text(theme::TEXT_META, theme::muted())
                    .child(format!(
                        "{} / {width} × {height}{rate}",
                        format_length(info.duration)
                    ))
                    .into_any_element()
            }
            VideoProbe::Failed(error) => ui_text(theme::TEXT_BODY, theme::muted())
                .child(error.clone())
                .into_any_element(),
        };
        let spacing = match &import.probe {
            VideoProbe::Ready(info) => Some(format!(
                "Every {:.1} s",
                info.duration.as_secs_f64() / f64::from(import.count)
            )),
            _ => None,
        };
        let running = import.job.is_some();

        let frames_row =
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(div().w(px(72.0)).child(self.video_frames_field.clone()))
                .child(
                    div()
                        .flex()
                        .gap(px(2.0))
                        .child(mini_button("video-frames-fewer", assets::MINUS).on_click(
                            cx.listener(|this, _, _, cx| this.nudge_video_frames(-1, cx)),
                        ))
                        .child(mini_button("video-frames-more", assets::PLUS).on_click(
                            cx.listener(|this, _, _, cx| this.nudge_video_frames(1, cx)),
                        )),
                )
                .children(
                    spacing
                        .map(|spacing| mono_text(theme::TEXT_META, theme::faint()).child(spacing)),
                );

        let sizes = div()
            .grid()
            .grid_cols(3)
            .gap(px(4.0))
            .children(SIZES.map(|(size, text)| {
                segment_button(
                    ElementId::Name(SharedString::from(format!("video-size-{text}"))),
                    text,
                    size == import.size,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(import) = this.video_import.as_mut() {
                        import.size = size;
                        cx.notify();
                    }
                }))
            }));

        let progress = import.job.as_ref().map(|job| {
            let done = job.done.load(Ordering::Relaxed);
            let fraction = done as f32 / job.total.max(1) as f32;
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .relative()
                        .h(px(4.0))
                        .w_full()
                        .bg(theme::rule())
                        .child(
                            div()
                                .absolute()
                                .top(px(0.0))
                                .left(px(0.0))
                                .h_full()
                                .w(relative(fraction.clamp(0.0, 1.0)))
                                .bg(theme::ink()),
                        ),
                )
                .child(
                    mono_text(theme::TEXT_META, theme::muted())
                        .child(format!("Extracting {done} / {}", job.total)),
                )
        });

        let field = |title: &'static str, control: AnyElement| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(label(title))
                .child(control)
        };
        let ready = matches!(import.probe, VideoProbe::Ready(_)) && !running;
        let panel = dialog_panel(480.0)
            .debug_selector(|| "video-import".into())
            .child(dialog_header(
                "Import video",
                name,
                icon_button("video-import-close", assets::X)
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss_video_import(cx))),
            ))
            .child(
                div()
                    .px(px(20.0))
                    .py(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(summary)
                    .child(field("Frames", frames_row.into_any_element()))
                    .child(field("Size", sizes.into_any_element()))
                    .child(field(
                        "Folder",
                        mono_text(theme::TEXT_META, theme::muted())
                            .truncate()
                            .child(display_folder(&import.folder))
                            .into_any_element(),
                    ))
                    .children(progress)
                    .children(import.error.map(|error| {
                        ui_text(theme::TEXT_BODY, theme::muted())
                            .line_height(px(18.0))
                            .child(error)
                    })),
            )
            .child(
                dialog_footer()
                    .child(
                        button("video-import-cancel", "Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.dismiss_video_import(cx))),
                    )
                    .child(if ready {
                        primary_button("video-import-start", "Extract frames")
                            .on_click(cx.listener(|this, _, _, cx| this.start_video_extract(cx)))
                            .into_any_element()
                    } else {
                        disabled_button("Extract frames").into_any_element()
                    }),
            );
        Some(modal(
            panel,
            cx.listener(|this, _, _, cx| {
                // A stray click outside must not throw away a running job.
                if this
                    .video_import
                    .as_ref()
                    .is_some_and(|import| import.job.is_none())
                {
                    this.video_import = None;
                    cx.notify();
                }
            }),
        ))
    }
}

/// The frames folder and its parent: enough to recognise where it is.
fn display_folder(folder: &Path) -> String {
    let name = folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    folder.parent().and_then(Path::file_name).map_or_else(
        || name.clone(),
        |parent| format!("{} / {name}", parent.to_string_lossy()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Dismiss;

    #[gpui::test]
    fn the_dialog_reads_the_clip_and_steps_the_frame_count(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| assets::install_fonts(cx));
        let (view, cx) = cx.add_window_view(|_, cx| ContactSheet::new(cx));
        let clip =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../slate-core/tests/fixtures/clip.mp4");
        view.update(cx, |view, cx| view.open_video_import(clip, cx));
        for _ in 0..50 {
            cx.run_until_parked();
            let ready = view.update(cx, |view, _| {
                view.video_import
                    .as_ref()
                    .is_some_and(|import| matches!(import.probe, VideoProbe::Ready(_)))
            });
            if ready {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        view.update(cx, |view, cx| {
            let import = view.video_import.as_ref().unwrap();
            let VideoProbe::Ready(info) = import.probe else {
                panic!("the clip did not probe: {:?}", import.probe);
            };
            assert!((info.duration.as_secs_f64() - 4.0).abs() < 0.05);
            assert!(import.folder.ends_with("clip frames"));
            view.nudge_video_frames(1, cx);
            assert_eq!(
                view.video_import.as_ref().unwrap().count,
                video::DEFAULT_FRAMES + 1
            );
            assert_eq!(view.video_frames_field.read(cx).value(), "13");
            view.set_video_frames_text("5000", cx);
            assert_eq!(view.video_import.as_ref().unwrap().count, video::MAX_FRAMES);
        });
        cx.dispatch_action(Dismiss);
        view.update(cx, |view, _| assert!(view.video_import.is_none()));
    }

    #[test]
    fn lengths_read_as_minutes_or_hours() {
        assert_eq!(format_length(Duration::from_millis(62_400)), "1:02.4");
        assert_eq!(format_length(Duration::from_secs(3723)), "1:02:03");
        assert_eq!(format_length(Duration::from_millis(4_000)), "0:04.0");
    }
}
