//! The row-height value types shared by the list geometry service: a height
//! that is positive and finite by construction, and a height tagged with how
//! far it can be trusted.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct RowHeight(f64);

impl RowHeight {
    pub(in crate::ui) fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }

    /// A height authored as a whole number of pixels. For `const` items: a
    /// non-positive value fails the build, so no caller has to handle one.
    pub(in crate::ui) const fn from_authored_pixels(pixels: i32) -> Self {
        assert!(pixels > 0, "an authored row height must be positive");
        Self(pixels as f64)
    }

    pub(in crate::ui) fn pixels(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum RowHeightSource {
    Assumed,
    Measured,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct TrustedRowHeight {
    pub(in crate::ui) height: RowHeight,
    pub(in crate::ui) source: RowHeightSource,
}

impl TrustedRowHeight {
    pub(in crate::ui) fn assumed(height: RowHeight) -> Self {
        Self {
            height,
            source: RowHeightSource::Assumed,
        }
    }

    pub(in crate::ui) fn measured(height: RowHeight) -> Self {
        Self {
            height,
            source: RowHeightSource::Measured,
        }
    }

    pub(in crate::ui) fn from_cache(value: f64) -> Option<Self> {
        if value == 0.0 {
            return None;
        }
        let source = if value.is_sign_negative() {
            RowHeightSource::Assumed
        } else {
            RowHeightSource::Measured
        };
        RowHeight::new(value.abs()).map(|height| Self { height, source })
    }

    pub(in crate::ui) fn cache_value(self) -> f64 {
        match self.source {
            RowHeightSource::Assumed => -self.height.pixels(),
            RowHeightSource::Measured => self.height.pixels(),
        }
    }
}
