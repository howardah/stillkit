pub(crate) mod decoding;
pub(crate) mod image;
pub(crate) mod metadata;

pub(crate) type ToolReporter<'a> = &'a (dyn Fn(&str) + Sync);

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum Pipeline {
    #[default]
    Auto,
    BuiltIn,
}

impl Pipeline {
    pub(crate) fn from_no_deps(no_deps: bool) -> Self {
        if no_deps { Self::BuiltIn } else { Self::Auto }
    }

    pub(crate) fn allows_tools(self) -> bool {
        matches!(self, Self::Auto)
    }
}
