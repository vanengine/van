(function() {
  "use strict";

  var context = null;

  function signal(value) {
    var subs = [];
    var s = {
      get value() {
        if (context && subs.indexOf(context) === -1) subs.push(context);
        return value;
      },
      set value(v) {
        if (v === value) return;
        value = v;
        var toRun = subs.slice();
        for (var i = 0; i < toRun.length; i++) toRun[i]();
      },
      peek: function() { return value; }
    };
    return s;
  }

  function computed(fn) {
    var s = signal(undefined);
    effect(function() { s.value = fn(); });
    return { get value() { return s.value; }, peek: function() { return s.peek(); } };
  }

  function effect(fn) {
    var run = function() {
      var prev = context;
      context = run;
      try { fn(); } finally { context = prev; }
    };
    run();
  }

  var batchQueue = null;

  function batch(fn) {
    if (batchQueue) { fn(); return; }
    batchQueue = [];
    try {
      fn();
    } finally {
      var q = batchQueue;
      batchQueue = null;
      for (var i = 0; i < q.length; i++) q[i]();
    }
  }

  function transition(el, show, name) {
    var p = name || 'v';
    if (!el.__van_t) { el.__van_t = true; el.style.display = show ? '' : 'none'; return; }
    if (show) {
      el.style.display = '';
      el.classList.add(p + '-enter-from', p + '-enter-active');
      requestAnimationFrame(function() { requestAnimationFrame(function() {
        el.classList.remove(p + '-enter-from');
        el.classList.add(p + '-enter-to');
        var done = function() {
          el.classList.remove(p + '-enter-active', p + '-enter-to');
          el.removeEventListener('transitionend', done);
        };
        el.addEventListener('transitionend', done);
      }); });
    } else {
      el.classList.add(p + '-leave-from', p + '-leave-active');
      requestAnimationFrame(function() { requestAnimationFrame(function() {
        el.classList.remove(p + '-leave-from');
        el.classList.add(p + '-leave-to');
        var done = function() {
          el.classList.remove(p + '-leave-active', p + '-leave-to');
          el.style.display = 'none';
          el.removeEventListener('transitionend', done);
        };
        el.addEventListener('transitionend', done);
      }); });
    }
  }

  function watch(source, fn) {
    var prev;
    var first = true;
    effect(function() {
      var val = typeof source === 'function' ? source() : source.value;
      if (!first) { fn(val, prev); }
      prev = val;
      first = false;
    });
  }

  window.Van = {
    signal: signal,
    computed: computed,
    effect: effect,
    batch: batch,
    transition: transition,
    watch: watch
  };
})();
