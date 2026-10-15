use crate::term::BufWrite as _;

/// Represents a foreground or background color for cells.
#[derive(Eq, PartialEq, Debug, Copy, Clone, Default)]
pub enum Color {
    /// The default terminal color.
    #[default]
    Default,

    /// An indexed terminal color.
    Idx(u8),

    /// An RGB terminal color. The parameters are (red, green, blue).
    Rgb(u8, u8, u8),
}

/// OSC 8 targets interned for the whole process, so a cell's id means the same target in
/// every screen and every clone; id `n` is `urls[n - 1]`.
#[derive(Default)]
struct Links {
    urls: Vec<std::sync::Arc<str>>,
    ids: std::collections::HashMap<std::sync::Arc<str>, u16>,
}

fn links() -> std::sync::MutexGuard<'static, Links> {
    static LINKS: std::sync::OnceLock<std::sync::Mutex<Links>> =
        std::sync::OnceLock::new();
    LINKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ponytail: ids are never freed, so after 65535 distinct targets in one process new links
// are drawn as plain text; reclaim ids no cell holds if that ever happens.
pub(crate) fn intern_link(uri: &str) -> u16 {
    let mut links = links();
    if let Some(&id) = links.ids.get(uri) {
        return id;
    }
    let Ok(id) = u16::try_from(links.urls.len() + 1) else {
        return 0;
    };
    let uri: std::sync::Arc<str> = uri.into();
    links.urls.push(uri.clone());
    links.ids.insert(uri, id);
    id
}

pub(crate) fn link_target(id: u16) -> Option<std::sync::Arc<str>> {
    let index = usize::from(id.checked_sub(1)?);
    links().urls.get(index).cloned()
}

const TEXT_MODE_INTENSITY: u8 = 0b0000_0011;
const TEXT_MODE_BOLD: u8 = 0b0000_0001;
const TEXT_MODE_DIM: u8 = 0b0000_0010;
const TEXT_MODE_ITALIC: u8 = 0b0000_0100;
const TEXT_MODE_UNDERLINE: u8 = 0b0000_1000;
const TEXT_MODE_INVERSE: u8 = 0b0001_0000;

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Attrs {
    pub fgcolor: Color,
    pub bgcolor: Color,
    /// The OSC 8 hyperlink the text belongs to, an id in the screen's link table, 0 for none.
    /// SGR leaves it alone; only OSC 8 and a full reset change it.
    pub link: u16,
    pub mode: u8,
}

impl Attrs {
    pub fn bold(&self) -> bool {
        self.mode & TEXT_MODE_BOLD != 0
    }

    pub fn dim(&self) -> bool {
        self.mode & TEXT_MODE_DIM != 0
    }

    fn intensity(&self) -> u8 {
        self.mode & TEXT_MODE_INTENSITY
    }

    pub fn set_bold(&mut self) {
        self.mode &= !TEXT_MODE_INTENSITY;
        self.mode |= TEXT_MODE_BOLD;
    }

    pub fn set_dim(&mut self) {
        self.mode &= !TEXT_MODE_INTENSITY;
        self.mode |= TEXT_MODE_DIM;
    }

    pub fn set_normal_intensity(&mut self) {
        self.mode &= !TEXT_MODE_INTENSITY;
    }

    pub fn italic(&self) -> bool {
        self.mode & TEXT_MODE_ITALIC != 0
    }

    pub fn set_italic(&mut self, italic: bool) {
        if italic {
            self.mode |= TEXT_MODE_ITALIC;
        } else {
            self.mode &= !TEXT_MODE_ITALIC;
        }
    }

    pub fn underline(&self) -> bool {
        self.mode & TEXT_MODE_UNDERLINE != 0
    }

    pub fn set_underline(&mut self, underline: bool) {
        if underline {
            self.mode |= TEXT_MODE_UNDERLINE;
        } else {
            self.mode &= !TEXT_MODE_UNDERLINE;
        }
    }

    pub fn inverse(&self) -> bool {
        self.mode & TEXT_MODE_INVERSE != 0
    }

    pub fn set_inverse(&mut self, inverse: bool) {
        if inverse {
            self.mode |= TEXT_MODE_INVERSE;
        } else {
            self.mode &= !TEXT_MODE_INVERSE;
        }
    }

    pub fn write_escape_code_diff(
        &self,
        contents: &mut Vec<u8>,
        other: &Self,
    ) {
        if self.link != other.link {
            contents.extend_from_slice(b"\x1b]8;;");
            if let Some(url) = link_target(self.link) {
                contents.extend_from_slice(url.as_bytes());
            }
            contents.extend_from_slice(b"\x1b\\");
        }
        if self != other && self == &Self::default() {
            crate::term::ClearAttrs.write_buf(contents);
            return;
        }

        let attrs = crate::term::Attrs::default();

        let attrs = if self.fgcolor == other.fgcolor {
            attrs
        } else {
            attrs.fgcolor(self.fgcolor)
        };
        let attrs = if self.bgcolor == other.bgcolor {
            attrs
        } else {
            attrs.bgcolor(self.bgcolor)
        };
        let attrs = if self.intensity() == other.intensity() {
            attrs
        } else {
            attrs.intensity(match self.intensity() {
                0 => crate::term::Intensity::Normal,
                TEXT_MODE_BOLD => crate::term::Intensity::Bold,
                TEXT_MODE_DIM => crate::term::Intensity::Dim,
                _ => unreachable!(),
            })
        };
        let attrs = if self.italic() == other.italic() {
            attrs
        } else {
            attrs.italic(self.italic())
        };
        let attrs = if self.underline() == other.underline() {
            attrs
        } else {
            attrs.underline(self.underline())
        };
        let attrs = if self.inverse() == other.inverse() {
            attrs
        } else {
            attrs.inverse(self.inverse())
        };

        attrs.write_buf(contents);
    }
}
