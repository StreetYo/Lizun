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

/** Opening: measured from the reference recording, about 7% overshoot. */
export const OPEN_SPRING: SpringParams = { response: 0.5, damping: 0.65 };
/** Closing: quicker and without bounce. */
export const CLOSE_SPRING: SpringParams = { response: 0.34, damping: 1 };

const SETTLE_DISTANCE = 0.05;
const SETTLE_VELOCITY = 0.5;

export class Spring {
  value: number;
  velocity = 0;
  target: number;
  params: SpringParams;

  constructor(value: number, params: SpringParams = OPEN_SPRING) {
    this.value = value;
    this.target = value;
    this.params = params;
  }

  setTarget(target: number, params: SpringParams = this.params): void {
    this.target = target;
    this.params = params;
  }

  get settled(): boolean {
    return Math.abs(this.value - this.target) < SETTLE_DISTANCE && Math.abs(this.velocity) < SETTLE_VELOCITY;
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
