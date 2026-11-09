//! Borrador de un campo numérico sincronizado con el valor del motor.
//!
//! El texto solo se reescribe cuando el valor externo cambia, para no borrar lo
//! que el usuario está escribiendo.

#[derive(Default)]
pub struct NumberDraft {
    pub text: String,
    last_value: f32,
}

impl NumberDraft {
    pub fn sync(&mut self, value: f32) {
        if self.text.is_empty() || (self.last_value - value).abs() > 0.0001 {
            self.text = format_number(value);
            self.last_value = value;
        }
    }

    pub fn parsed(&self) -> Option<f32> {
        self.text
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
    }
}

pub fn format_number(value: f32) -> String {
    if (value - value.round()).abs() < 0.0001 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_does_not_overwrite_an_edit() {
        let mut draft = NumberDraft::default();
        draft.sync(50.0);
        assert_eq!(draft.text, "50");
        draft.text = "12.5".to_string();
        draft.sync(50.0);
        assert_eq!(draft.text, "12.5");
        draft.sync(60.0);
        assert_eq!(draft.text, "60");
    }

    #[test]
    fn format_keeps_integers_clean() {
        assert_eq!(format_number(50.0), "50");
        assert_eq!(format_number(0.25), "0.25");
    }
}
