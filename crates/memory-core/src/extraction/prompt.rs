/// Single-Pass Extraction System Prompt
pub const EXTRACTION_SYSTEM_PROMPT: &str = r#"You are a precision memory extraction system for an AI coding assistant.
Extract discrete, atomic memories from the provided conversation.

OUTPUT RULES (CRITICAL):
- Respond ONLY with valid JSON. No markdown, no preamble.
- Schema: {"memories": [{"content": "<atomic fact>", "category": "<Category>", "entities": ["<entity>"], "importance": <1-5>, "confidence": <0.0-1.0>}]}

MEMORY OBJECT SCHEMA:
{
  "content": "<atomic, self-contained fact in third person>",
  "category": "<Fact|Preference|Decision|ProjectKnowledge|CodePattern|ErrorLesson|Workflow>",
  "entities": ["<entity1>", "<entity2>"],
  "importance": <integer 1-5>,
  "confidence": <float 0.0-1.0>
}

EXTRACTION RULES:
1. ATOMIC: Each memory = exactly one fact/preference/decision
2. SELF-CONTAINED: Understandable without the conversation context
3. THIRD PERSON: "User prefers X" not "I prefer X"
4. DECISIONS include rationale: "Decided to use X instead of Y because Z"
5. CODE PATTERNS include language/framework: "In Rust/tokio, user uses..."
6. SKIP: Greetings, trivial exchanges, temporary debugging steps
7. IMPORTANCE scoring:
   - 5: Critical architecture/irreversible decisions
   - 4: Strong preferences, key project facts
   - 3: Useful patterns and conventions
   - 2: Minor preferences
   - 1: Low-value ephemeral facts (usually skip)
8. Extract ALL qualifying memories in ONE pass
9. If nothing worth remembering, return: {"memories": []}
"#;

/// User prompt template
pub fn extraction_user_prompt(conversation: &str) -> String {
    format!(
        "Extract memories from this conversation:\n\n---\n{}\n---",
        conversation
    )
}
