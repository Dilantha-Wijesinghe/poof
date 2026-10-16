//! Interface icons from Lucide (ISC license, see `assets/icons/LICENSE.txt`),
//! embedded at compile time and tinted at render time.

use iced::Pixels;
use iced::widget::{Svg, svg};

macro_rules! icons {
    ($($variant:ident => $file:literal),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Icon { $($variant),* }

        impl Icon {
            fn bytes(self) -> &'static [u8] {
                match self {
                    $(Icon::$variant => include_bytes!(concat!("../assets/icons/", $file, ".svg"))),*
                }
            }
        }
    };
}

icons! {
    AlertTriangle => "triangle-alert",
    ArrowDown => "arrow-down-right",
    ArrowLeft => "arrow-left",
    ArrowUp => "arrow-up-right",
    Bell => "bell",
    Camera => "camera",
    Check => "check",
    ChevronRight => "chevron-right",
    Clock => "clock",
    Disk => "hard-drive",
    ExternalLink => "external-link",
    File => "file",
    FileBox => "file-box",
    Folder => "folder",
    FolderOpen => "folder-open",
    Gauge => "gauge",
    Grid => "layout-grid",
    History => "history",
    Info => "info",
    Lock => "lock",
    Refresh => "rotate-cw",
    Scan => "scan",
    Search => "search",
    Settings => "settings",
    Sparkles => "sparkles",
    Stop => "circle-stop",
    Trash => "trash-2",
    X => "x",
}

impl Icon {
    /// A square icon of `size` px, tinted with the theme's muted text colour.
    /// Chain `.style(...)` to recolour it.
    pub fn view<'a>(self, size: impl Into<Pixels>) -> Svg<'a> {
        let size = size.into();
        svg(svg::Handle::from_memory(self.bytes()))
            .width(size)
            .height(size)
            .style(crate::style::icon(|t| t.muted))
    }
}
