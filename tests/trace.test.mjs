import test from "node:test";
import assert from "node:assert/strict";
import { checkTrace } from "../scripts/check-trace.mjs";
const stage = { type: "stage", id: "host-1" },
  sim = { type: "simulate", id: "host-1", ok: true },
  approval = { type: "approval", id: "host-1", ok: true },
  commit = { type: "commit", id: "host-1" },
  confirmed = {
    type: "confirmed",
    id: "host-1",
    signature: "recorded-host-signature",
  };
test("accepts an ordered authorized host execution", () =>
  assert.deepEqual(
    checkTrace([
      stage,
      sim,
      approval,
      commit,
      confirmed,
      { type: "completion_claim", id: "host-1" },
    ]),
    [],
  ));
test("failed simulation prevents commit", () =>
  assert.ok(
    checkTrace([stage, { ...sim, ok: false }, approval, commit]).length,
  ));
test("rejected approval prevents commit", () =>
  assert.ok(
    checkTrace([stage, sim, { ...approval, ok: false }, commit]).length,
  ));
test("duplicate callbacks do not authorize a second commit", () =>
  assert.ok(
    checkTrace([
      stage,
      sim,
      approval,
      commit,
      confirmed,
      confirmed,
      commit,
    ]).includes("Duplicate commit"),
  ));
test("submission is not confirmation", () =>
  assert.ok(
    checkTrace([
      stage,
      sim,
      approval,
      commit,
      { type: "completion_claim", id: "host-1" },
    ]).length,
  ));
test("specific venue must actually be called", () =>
  assert.ok(
    checkTrace(
      [{ type: "tool", name: "jupiter_get_quote" }],
      ["dottie_fluxbeam_quote"],
    ).length,
  ));
