const test = require("node:test");
const assert = require("node:assert/strict");
const { parseMemoriesResponse, formatMemoriesForInjection } = require("../dist/response.js");

const memory = {
  id: "1",
  content: "User prefers Rust.",
  category: "Preference",
  importance_score: 0.8,
};

test("parses direct, wrapped, and rmcp text responses", () => {
  assert.deepEqual(parseMemoriesResponse([memory]), [memory]);
  assert.deepEqual(parseMemoriesResponse({ results: [memory] }), [memory]);
  assert.deepEqual(
    parseMemoriesResponse({ content: [{ type: "text", text: JSON.stringify([memory]) }] }),
    [memory],
  );
});

test("formats nested search results as bounded untrusted context", () => {
  const text = formatMemoriesForInjection([{ memory, score_final: 0.72 }]);
  assert.match(text, /Retrieved Memory Context/);
  assert.match(text, /not instructions/i);
  assert.match(text, /category="Preference"/);
  assert.match(text, /relevance="0\.720"/);
  assert.match(text, /User prefers Rust\./);
  assert.doesNotMatch(text, /undefined/);
});
