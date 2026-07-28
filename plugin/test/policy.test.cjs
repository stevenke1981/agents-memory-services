const test = require("node:test");
const assert = require("node:assert/strict");

const {
  capturePriority,
  redactSecrets,
  selectMemories,
  shouldCaptureTurn,
  shouldRecall,
} = require("../dist/index.js");

test("keeps explicit preferences and durable implementation outcomes", () => {
  assert.equal(capturePriority("Remember that I always use Rust for core services.", "Understood."), 3);
  assert.equal(
    shouldCaptureTurn(
      "Fix the SQLite migration regression in this project.",
      "Fixed the root cause and added a passing regression test.",
    ),
    true,
  );
});

test("skips low-signal lifecycle traffic", () => {
  assert.equal(shouldRecall("thanks"), false);
  assert.equal(shouldCaptureTurn("ok", "Done."), false);
});

test("redacts bearer tokens, JWTs, credentials, and private keys", () => {
  const redacted = redactSecrets(
    [
      "Authorization: Bearer abcdefghijklmnopqrstuvwxyz123456",
      "jwt=eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.signaturevalue",
      "postgres://admin:supersecret@localhost/app",
      "-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----",
    ].join("\n"),
  );

  assert.doesNotMatch(redacted, /abcdefghijklmnopqrstuvwxyz123456/);
  assert.doesNotMatch(redacted, /eyJhbGci/);
  assert.doesNotMatch(redacted, /admin:supersecret/);
  assert.doesNotMatch(redacted, /\nsecret\n/);
});

test("deduplicates recall and keeps the strongest relevant copy", () => {
  const selected = selectMemories([
    {
      id: "low",
      content: "Use Rust for core services",
      category: "Preference",
      importance_score: 0.8,
      score_final: 0.35,
    },
    {
      id: "high",
      content: "Use Rust for core services",
      category: "Preference",
      importance_score: 0.9,
      score_final: 0.8,
    },
    {
      id: "irrelevant",
      content: "Unrelated",
      category: "Fact",
      importance_score: 0.2,
      score_final: 0.1,
    },
  ]);

  assert.deepEqual(selected.map((memory) => memory.id), ["high"]);
});
