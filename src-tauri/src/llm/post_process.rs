//! Deterministic post-processing applied to the final polished text right
//! before it is typed into the target application. The capsule still shows
//! the streamed preview; only the single final insertion goes through here.
//!
//! Identity for now; M3 adds Simplified->Traditional conversion and exact
//! correction-rule replacement.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostProcessOptions {}

pub fn post_process_final_text(text: &str, _options: &PostProcessOptions) -> String {
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_by_default() {
        let input = "第一，買牛奶\n第二，洗衣服";
        assert_eq!(
            post_process_final_text(input, &PostProcessOptions::default()),
            input
        );
    }
}
