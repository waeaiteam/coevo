import { describe, it, expect } from "vitest";
import { inferTrackFromIntent } from "../utils/trackInference";

describe("trackInference", () => {
  it('returns red for "production database rollback"', () => {
    const r = inferTrackFromIntent("production database rollback");
    expect(r.track).toBe("red");
    expect(r.reason).toContain("production");
  });

  it('returns yellow for "send staging notification"', () => {
    const r = inferTrackFromIntent("send staging notification");
    expect(r.track).toBe("yellow");
    expect(r.reason).toContain("notification");
  });

  it("returns green for ordinary read intent", () => {
    const r = inferTrackFromIntent("read metrics and analyze logs");
    expect(r.track).toBe("green");
  });

  it("returns red for critical P1 emergency", () => {
    const r = inferTrackFromIntent("critical P1 emergency fix needed");
    expect(r.track).toBe("red");
  });

  it("returns red for payment processing", () => {
    expect(inferTrackFromIntent("process customer payment").track).toBe("red");
  });

  it("returns yellow for deploy notification", () => {
    expect(inferTrackFromIntent("deploy update to staging").track).toBe("yellow");
  });

  it("returns green for analysis only", () => {
    expect(inferTrackFromIntent("analyze system health").track).toBe("green");
  });

  it("returns red for drop table", () => {
    expect(inferTrackFromIntent("drop table users").track).toBe("red");
  });
});
