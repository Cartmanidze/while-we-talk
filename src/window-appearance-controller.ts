export interface WindowAppearance { hideFromCapture: boolean; windowOpacity: number }
export interface WindowAppearanceState extends WindowAppearance { saving: boolean }
type Request = { value: WindowAppearance };
const same = (left: WindowAppearance, right: WindowAppearance) => left.hideFromCapture === right.hideFromCapture && left.windowOpacity === right.windowOpacity;
function valid(value: WindowAppearance): WindowAppearance {
  if (typeof value.hideFromCapture !== 'boolean' || !Number.isInteger(value.windowOpacity) || value.windowOpacity < 30 || value.windowOpacity > 100) {
    throw new Error('Непрозрачность окна должна быть от 30 до 100%.');
  }
  return { hideFromCapture: value.hideFromCapture, windowOpacity: value.windowOpacity };
}
export class WindowAppearanceController {
  private readonly send: (value: WindowAppearance) => Promise<boolean>;
  private readonly changed: (state: WindowAppearanceState) => void;
  private value: WindowAppearance;
  private confirmed: WindowAppearance;
  private observed: WindowAppearance;
  private awaitingEcho: WindowAppearance | undefined;
  private queued: Request | undefined;
  private inFlight: Request | undefined;
  private saving = false;
  constructor(initial: WindowAppearance, send: (value: WindowAppearance) => Promise<boolean>, changed: (state: WindowAppearanceState) => void) {
    this.value = valid(initial); this.confirmed = valid(initial); this.observed = valid(initial);
    this.send = send; this.changed = changed;
  }
  edit(value: WindowAppearance) { this.value = valid(value); this.publish(); }
  sync(value: WindowAppearance) {
    this.observed = valid(value);
    if (this.saving) return;
    // An older app-state event can arrive after invoke resolves; wait for the confirmed value.
    if (this.awaitingEcho && !same(value, this.awaitingEcho)) return;
    this.awaitingEcho = undefined;
    const editing = !same(this.value, this.confirmed);
    this.confirmed = valid(value);
    if (!editing) this.value = valid(value);
    this.publish();
  }
  async commit(force = false): Promise<void> {
    if (this.inFlight && same(this.value, this.inFlight.value)) { this.queued = undefined; return; }
    if (!this.saving && !force && same(this.value, this.confirmed)) return;
    this.queued = { value: { ...this.value } };
    if (this.saving) return;
    this.saving = true; this.publish();
    try {
      while (this.queued) {
        const request = this.queued; this.queued = undefined;
        this.inFlight = request;
        let accepted = false;
        try { accepted = await this.send(request.value); } catch { accepted = false; }
        if (accepted) {
          this.confirmed = request.value;
          this.awaitingEcho = same(this.observed, request.value) ? undefined : request.value;
        } else {
          if (same(this.value, request.value)) this.value = { ...this.confirmed };
        }
        this.inFlight = undefined;
      }
    } finally { this.inFlight = undefined; this.saving = false; this.publish(); }
  }
  snapshot(): WindowAppearanceState { return { ...this.value, saving: this.saving }; }
  private publish() { this.changed(this.snapshot()); }
}
