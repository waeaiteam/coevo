//! Static model pricing estimates.
//!
//! All prices are in **USD per 1K tokens** and are *estimates* derived from
//! public provider list prices, last reviewed **2026-06-12**. Providers
//! change prices without notice; these values are intended for cost
//! *estimation* and budgeting UI, never for billing. Cache-discounted,
//! batch, or long-context premium tiers are not modeled.
//!
//! Matching is by case-insensitive model-id family (prefix/substring), with
//! more specific families listed before generic ones. Known-free local
//! runtimes (ollama / llama / local) return `Some` with `0.0` rates so that
//! callers can distinguish "free" from "unknown" (`None`).

/// Estimated unit price for a model family, in USD per 1K tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    pub input_per_1k_usd: f64,
    pub output_per_1k_usd: f64,
}

/// Ordered pricing table: `(family patterns, input $/1K, output $/1K)`.
/// First matching row wins, so specific families must precede generic ones
/// (e.g. `gpt-4o-mini` before `gpt-4o`, `claude-opus-4-1` before
/// `claude-opus-4`).
const PRICING_TABLE: &[(&[&str], f64, f64)] = &[
    // --- Known-free local runtimes -------------------------------------
    (&["ollama", "llama", "local"], 0.0, 0.0),
    // --- DeepSeek (unified V3.2 pricing, cache-miss input) -------------
    (&["deepseek-reasoner", "deepseek-r1"], 0.000_28, 0.000_42),
    (
        &["deepseek-v4", "deepseek-chat", "deepseek-v3"],
        0.000_28,
        0.000_42,
    ),
    // --- OpenAI ---------------------------------------------------------
    (&["gpt-4o-mini"], 0.000_15, 0.000_6),
    (&["gpt-4o"], 0.002_5, 0.01),
    (&["gpt-4.1-nano"], 0.000_1, 0.000_4),
    (&["gpt-4.1-mini"], 0.000_4, 0.001_6),
    (&["gpt-4.1"], 0.002, 0.008),
    (&["o4-mini"], 0.001_1, 0.004_4),
    (&["o3-mini"], 0.001_1, 0.004_4),
    (&["o3"], 0.002, 0.008),
    // --- Anthropic -------------------------------------------------------
    (&["claude-3-5-sonnet"], 0.003, 0.015),
    (&["claude-3-5-haiku"], 0.000_8, 0.004),
    (&["claude-3-7-sonnet"], 0.003, 0.015),
    (&["claude-sonnet-4"], 0.003, 0.015),
    // Opus 4.0/4.1 were $15/$75 per MTok; Opus 4.5+ dropped to $5/$25.
    (
        &["claude-opus-4-1", "claude-opus-4-0", "claude-opus-4-2025"],
        0.015,
        0.075,
    ),
    (&["claude-opus-4"], 0.005, 0.025),
    (&["claude-haiku-4"], 0.001, 0.005),
    // --- Google Gemini ---------------------------------------------------
    (&["gemini-2.0-flash"], 0.000_1, 0.000_4),
    (&["gemini-2.5-pro"], 0.001_25, 0.01),
    // --- Alibaba Qwen ----------------------------------------------------
    (&["qwen-max"], 0.001_6, 0.006_4),
    (&["qwen-turbo"], 0.000_05, 0.000_2),
    (&["qwen-plus", "qwen"], 0.000_4, 0.001_2),
    // --- Zhipu GLM ---------------------------------------------------------
    (&["glm-4"], 0.000_6, 0.002_2),
    // --- Moonshot / Kimi ---------------------------------------------------
    (&["kimi", "moonshot"], 0.000_6, 0.002_5),
];

/// Returns the estimated unit pricing for `model_id`, matched by
/// case-insensitive family. `None` means the model is unknown; `Some` with
/// `0.0` rates means it is a known-free local model.
pub fn unit_cost(model_id: &str) -> Option<ModelPricing> {
    let id = model_id.trim().to_lowercase();
    if id.is_empty() {
        return None;
    }
    for (patterns, input_per_1k_usd, output_per_1k_usd) in PRICING_TABLE {
        if patterns.iter().any(|pattern| family_matches(&id, pattern)) {
            return Some(ModelPricing {
                input_per_1k_usd: *input_per_1k_usd,
                output_per_1k_usd: *output_per_1k_usd,
            });
        }
    }
    None
}

/// Estimated total cost in USD for a single call, or `None` for unknown
/// models. Free local models return `Some(0.0)`.
pub fn estimate_cost_usd(
    model_id: &str,
    prompt_tokens: u64,
    completion_tokens: u64,
) -> Option<f64> {
    let pricing = unit_cost(model_id)?;
    Some(
        (prompt_tokens as f64 / 1000.0) * pricing.input_per_1k_usd
            + (completion_tokens as f64 / 1000.0) * pricing.output_per_1k_usd,
    )
}

/// Family match: prefix match always counts; substring match only for
/// patterns long enough (> 4 chars) to be unambiguous. Very short families
/// such as `o3` must appear as a prefix so they cannot fire inside
/// unrelated ids.
fn family_matches(id: &str, pattern: &str) -> bool {
    id.starts_with(pattern) || (pattern.len() > 4 && id.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(model_id: &str) -> (f64, f64) {
        let p = unit_cost(model_id).unwrap_or_else(|| panic!("expected pricing for {model_id}"));
        (p.input_per_1k_usd, p.output_per_1k_usd)
    }

    #[test]
    fn matches_deepseek_family() {
        assert_eq!(rate("deepseek-chat"), (0.000_28, 0.000_42));
        assert_eq!(rate("deepseek-reasoner"), (0.000_28, 0.000_42));
        assert_eq!(rate("DeepSeek-V3"), (0.000_28, 0.000_42));
    }

    #[test]
    fn specific_openai_families_win_over_generic() {
        assert_eq!(rate("gpt-4o-mini"), (0.000_15, 0.000_6));
        assert_eq!(rate("gpt-4o-2024-11-20"), (0.002_5, 0.01));
        assert_eq!(rate("gpt-4.1-nano"), (0.000_1, 0.000_4));
        assert_eq!(rate("gpt-4.1"), (0.002, 0.008));
        assert_eq!(rate("o3-mini"), (0.001_1, 0.004_4));
        assert_eq!(rate("o3"), (0.002, 0.008));
        assert_eq!(rate("o4-mini-2025-04-16"), (0.001_1, 0.004_4));
    }

    #[test]
    fn short_families_require_prefix_match() {
        // "o3" appears inside this fictional id but must not match it.
        assert!(unit_cost("provider-o3-clone").is_none());
    }

    #[test]
    fn matches_anthropic_families() {
        assert_eq!(rate("claude-3-5-sonnet-20241022"), (0.003, 0.015));
        assert_eq!(rate("claude-3-5-haiku-20241022"), (0.000_8, 0.004));
        assert_eq!(rate("claude-3-7-sonnet-20250219"), (0.003, 0.015));
        assert_eq!(rate("claude-sonnet-4-6"), (0.003, 0.015));
        assert_eq!(rate("claude-opus-4-1"), (0.015, 0.075));
        assert_eq!(rate("claude-opus-4-6"), (0.005, 0.025));
        assert_eq!(rate("claude-haiku-4-5"), (0.001, 0.005));
    }

    #[test]
    fn matches_gemini_qwen_glm_kimi_families() {
        assert_eq!(rate("gemini-2.0-flash"), (0.000_1, 0.000_4));
        assert_eq!(rate("gemini-2.5-pro"), (0.001_25, 0.01));
        assert_eq!(rate("qwen-max-latest"), (0.001_6, 0.006_4));
        assert_eq!(rate("qwen-plus"), (0.000_4, 0.001_2));
        assert_eq!(rate("qwen-turbo"), (0.000_05, 0.000_2));
        assert_eq!(rate("glm-4-plus"), (0.000_6, 0.002_2));
        assert_eq!(rate("kimi-k2"), (0.000_6, 0.002_5));
        assert_eq!(rate("moonshot-v1-8k"), (0.000_6, 0.002_5));
    }

    #[test]
    fn local_models_are_free_not_unknown() {
        assert_eq!(rate("llama3.1:8b"), (0.0, 0.0));
        assert_eq!(rate("ollama/qwen2.5"), (0.0, 0.0));
        assert_eq!(rate("local-model"), (0.0, 0.0));
        assert_eq!(estimate_cost_usd("llama3.1:8b", 10_000, 10_000), Some(0.0));
    }

    #[test]
    fn unknown_models_return_none() {
        assert!(unit_cost("totally-unknown-model").is_none());
        assert!(unit_cost("").is_none());
        assert!(estimate_cost_usd("totally-unknown-model", 1000, 1000).is_none());
    }

    #[test]
    fn estimates_combine_prompt_and_completion_rates() {
        // deepseek-chat: 0.00028 in / 0.00042 out per 1K.
        let cost = estimate_cost_usd("deepseek-chat", 10_000, 5_000).unwrap();
        let expected = 10.0 * 0.000_28 + 5.0 * 0.000_42;
        assert!((cost - expected).abs() < 1e-12, "cost was {cost}");
    }
}
