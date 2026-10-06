export class Counter {
    value = $state(2);
    doubled = $derived(this.value * 2);

    increment() { this.value++; }

    snapshot() { return $state.snapshot({ value: this.value, doubled: this.doubled }); }
}
