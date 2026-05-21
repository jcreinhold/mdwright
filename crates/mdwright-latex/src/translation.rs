use crate::error::SourceSpan;

/// Loss marker recorded when a source translation cannot be exact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationLoss {
    span: SourceSpan,
    reason: String,
}

impl TranslationLoss {
    /// Source span where the loss occurred.
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        self.span
    }

    /// Why translation was lossy.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Source translation output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translation {
    text: String,
    losses: Vec<TranslationLoss>,
}

impl Translation {
    /// Translated source text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Losses recorded during translation.
    #[must_use]
    pub fn losses(&self) -> &[TranslationLoss] {
        &self.losses
    }

    /// Whether translation was exact.
    #[must_use]
    pub fn is_lossless(&self) -> bool {
        self.losses.is_empty()
    }
}
