/**
 * Damped spring solved analytically, so it behaves the same at any frame rate and keeps its
 * velocity when the target changes mid-flight (e.g. the cursor leaves while opening).
 *
 * Parameters follow SwiftUI's convention: `response` is the period of the undamped spring in
 * seconds and `damping` is the damping fraction (1 = no overshoot).
 */
export interface SpringParams {
  response: number;
  damping: number;
}

/** A spring described the way SwiftUI does since iOS 17: how long it takes and how much it
 * bounces (0 = none, 0.3 = a clear bounce). */
export function spring(duration: number, bounce = 0): SpringParams {
  return { response: duration, damping: 1 - bounce };
}

// Island shape.
/** Opening: measured from the reference recording, about 7% overshoot. */
export const OPEN_SPRING = spring(0.5, 0.35);
/** Back to the mini island, and the mini island coming out or changing width: quicker, with a
 * small bounce. It stays within the hover margin around the capsule. */
export const CLOSE_SPRING = spring(0.4, 0.2);
/** The mini island going into the top edge: no bounce, there is nothing to bounce against. */
export const HIDE_SPRING = spring(0.42);

// Content: it fades in and sharpens a little behind the shape, and goes at once.
export const CONTENT_IN_SPRING = spring(0.45);
export const CONTENT_SCALE_IN_SPRING = spring(0.5, 0.3);
export const CONTENT_OUT_SPRING = spring(0.2);
/** Content of the mini island shrinking into the edge along with the shape. */
export const CONTENT_HIDE_SPRING = spring(0.36);

const SETTLE_DISTANCE = 0.05;
const SETTLE_VELOCITY = 0.5;

export class Spring {
  value: number;
  velocity = 0;
  target: number;
  params: SpringParams;
  private readonly precision: number;

  /** `precision` scales when the spring counts as settled: 1 suits pixels, 0.01 a 0–1 value. */
  constructor(value: number, params: SpringParams = OPEN_SPRING, precision = 1) {
    this.value = value;
    this.target = value;
    this.params = params;
    this.precision = precision;
  }

  setTarget(target: number, params: SpringParams = this.params): void {
    this.target = target;
    this.params = params;
  }

  get settled(): boolean {
    return (
      Math.abs(this.value - this.target) < SETTLE_DISTANCE * this.precision &&
      Math.abs(this.velocity) < SETTLE_VELOCITY * this.precision
    );
  }

  /** Advances the spring by `dt` seconds; returns true once it has come to rest. */
  step(dt: number): boolean {
    const omega = (2 * Math.PI) / this.params.response;
    const zeta = Math.min(this.params.damping, 1);
    const x0 = this.value - this.target;
    const v0 = this.velocity;
    let x: number;
    let v: number;

    if (zeta >= 0.999) {
      const decay = Math.exp(-omega * dt);
      const b = v0 + omega * x0;
      x = decay * (x0 + b * dt);
      v = decay * (b - omega * (x0 + b * dt));
    } else {
      const omegaD = omega * Math.sqrt(1 - zeta * zeta);
      const decay = Math.exp(-zeta * omega * dt);
      const b = (v0 + zeta * omega * x0) / omegaD;
      const cos = Math.cos(omegaD * dt);
      const sin = Math.sin(omegaD * dt);
      x = decay * (x0 * cos + b * sin);
      v = decay * ((b * omegaD - zeta * omega * x0) * cos - (x0 * omegaD + zeta * omega * b) * sin);
    }

    this.value = this.target + x;
    this.velocity = v;
    if (this.settled) {
      this.value = this.target;
      this.velocity = 0;
      return true;
    }
    return false;
  }
}
