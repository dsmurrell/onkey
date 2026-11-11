// npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { readChallenge, checkChallenge, challengeMessage, CHALLENGE_KEYS } from "../lib/challenge.ts";

// the app's test key and the signature its test makes with it (src-tauri/src/challenge.rs)
const TEST_KEY = "0460fed4ba255a9d31c961eb74c6356d68c049b8923b61fa6ce669622e60f29fb67903fe1008b8bc99a41ae9e95628bc64f2f1b20c2d7e9f5177a3c294d4462299";
const SIG = "F9tUas_rFTv-shV8UOEw3cT9aS1l9wK0_Re0rKqdkWjOi7iSx6gmwMXmC7Px8v2UED2_2kUILb53QfOxuwIKTQ";
const link = (score, name = "Daniel", sig = SIG) => `#song=Everybody%27s%20Changing&by=Keane&score=${score}&name=${name}${sig ? `&sig=${sig}` : ""}`;

test("a link is read: title, artist, score, name and signature", () => {
  assert.deepEqual(readChallenge(link(81)), { song: "Everybody's Changing", by: "Keane", score: 81, name: "Daniel", sig: SIG });
  assert.equal(readChallenge("#song=Yellow&score=91").sig, "");
  assert.equal(readChallenge("#song=Yellow&score=91&sig=<script>").sig, "");
  assert.equal(readChallenge("#song=Yellow&score=101"), null);
  assert.equal(readChallenge(""), null);
});

test("the app's signature fits the link it made", async () => {
  const c = readChallenge(link(81));
  assert.equal(challengeMessage(c), "onkey-challenge-v1\nEverybody's Changing\nKeane\n81\nDaniel");
  assert.equal(await checkChallenge(c, [TEST_KEY]), true);
  // (and with the real key listed first: any listed key will do)
  assert.equal(await checkChallenge(c, [...CHALLENGE_KEYS, TEST_KEY]), true);
});

test("a score, name or song changed in the address isn't checked, nor is a link without a signature", async () => {
  assert.equal(await checkChallenge(readChallenge(link(99)), [TEST_KEY]), false);
  assert.equal(await checkChallenge(readChallenge(link(81, "Dan")), [TEST_KEY]), false);
  assert.equal(await checkChallenge(readChallenge(link(81).replace("Changing", "Changin")), [TEST_KEY]), false);
  assert.equal(await checkChallenge(readChallenge(link(81, "Daniel", "")), [TEST_KEY]), false);
  assert.equal(await checkChallenge(readChallenge(link(81, "Daniel", "A".repeat(86))), [TEST_KEY]), false);
  // a signature made with another key (the test key isn't one the site trusts)
  assert.equal(await checkChallenge(readChallenge(link(81))), false);
});

test("the keys the site trusts are P-256 points", () => {
  for (const k of CHALLENGE_KEYS) assert.match(k, /^04[0-9a-f]{128}$/);
});
