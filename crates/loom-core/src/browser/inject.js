/**
 * Loom's page collector.
 *
 * Injected into every frame at document start, before the page's own scripts,
 * so it can patch the console and observe real input without the page being
 * able to see it first.
 *
 * Everything it can do asynchronously it does through `window.chrome.webview
 * .postMessage` (in-process, no port, no CDP). Tool calls arrive as
 * `window.__loomBrowser.handle(op, argsJson)` and return a JSON **string**,
 * because that is what `eval_with_callback` can carry back across every
 * platform without an exception escaping.
 *
 * The one rule that shapes all of it: a snapshot is text, not pixels. A page
 * costs a couple of hundred tokens to look at, not a couple of thousand.
 */
(function () {
  if (window.__loomBrowser) return;

  var USER_WINDOW_MS = 30000;

  /** Real input the user made in this frame, newest first. */
  var userEvents = [];
  /** Console output, ring-buffered. */
  var consoleLog = [];
  /** Uncaught errors and failed requests. */
  var failures = [];
  /** JS dialogs the page tried to open, queued for the model to answer. */
  var dialogs = [];
  /** Fields whose value came from real typing (not from a script, not from us). */
  var userTyped = Object.create(null);
  var lastUserAt = 0;
  /** Set while we are synthesising, so our own events are not recorded. */
  var injecting = false;

  var MAX_CONSOLE = 500;

  function now() {
    return Date.now();
  }

  function push(list, item, cap) {
    list.push(item);
    if (list.length > cap) list.splice(0, list.length - cap);
  }

  function str(value) {
    try {
      if (value instanceof Error) return value.name + ": " + value.message;
      if (typeof value === "string") return value;
      if (typeof value === "object" && value !== null) return JSON.stringify(value);
      return String(value);
    } catch (error) {
      return "[unserialisable]";
    }
  }

  /* ---------------------------------------------------------------- input */

  function noteUser(kind, detail) {
    lastUserAt = now();
    push(userEvents, { kind: kind, detail: detail, at: lastUserAt }, 200);
  }

  function describe(node) {
    if (!node || !node.tagName) return "";
    var out = node.tagName.toLowerCase();
    if (node.id) out += "#" + node.id;
    else if (node.name) out += '[name="' + node.name + '"]';
    return out;
  }

  ["keydown", "mousedown", "click", "input", "focusin", "wheel"].forEach(
    function (type) {
      window.addEventListener(
        type,
        function (event) {
          if (injecting || !event.isTrusted) return;
          var target = event.target;
          if (type === "input") {
            if (target && target.tagName) {
              userTyped[selectorFor(target)] = now();
            }
            noteUser("typed", describe(target) + " = " + clip(valueOf(target), 40));
            return;
          }
          if (type === "wheel") {
            noteUser("scrolled", "to y=" + Math.round(window.scrollY));
            return;
          }
          noteUser(type, describe(target) + labelOf(target));
        },
        true,
      );
    },
  );

  function clip(text, limit) {
    var value = text == null ? "" : String(text);
    return value.length > limit ? value.slice(0, limit) + "…" : value;
  }

  /* -------------------------------------------------------------- console */

  ["log", "info", "warn", "error", "debug"].forEach(function (level) {
    var original = console[level];
    console[level] = function () {
      var parts = [];
      for (var i = 0; i < arguments.length; i += 1) parts.push(str(arguments[i]));
      push(consoleLog, { level: level, text: clip(parts.join(" "), 600), at: now() }, MAX_CONSOLE);
      if (original) original.apply(console, arguments);
    };
  });

  window.addEventListener("error", function (event) {
    push(failures, { kind: "error", text: clip(str(event.message), 600), at: now() }, MAX_CONSOLE);
  });
  window.addEventListener("unhandledrejection", function (event) {
    push(failures, { kind: "rejection", text: clip(str(event.reason), 600), at: now() }, MAX_CONSOLE);
  });

  /* --------------------------------------------------------------- dialogs */

  // The page's own alert/confirm/prompt are replaced with a queue, so a modal
  // cannot freeze a turn. The model answers with `browser_dialog`.
  window.alert = function (message) {
    push(dialogs, { type: "alert", message: clip(str(message), 400), at: now() }, 20);
    return undefined;
  };
  window.confirm = function (message) {
    push(dialogs, { type: "confirm", message: clip(str(message), 400), at: now() }, 20);
    return false;
  };
  window.prompt = function (message, fallback) {
    push(
      dialogs,
      { type: "prompt", message: clip(str(message), 400), value: clip(str(fallback), 200), at: now() },
      20,
    );
    return null;
  };

  /* ------------------------------------------------------------- elements */

  var ACTIONABLE =
    'a[href],button,input,select,textarea,summary,[role],[contenteditable=""],' +
    '[contenteditable="true"],[tabindex]:not([tabindex="-1"])';

  function visible(node) {
    if (!node || !node.isConnected) return false;
    var rect = node.getBoundingClientRect();
    if (rect.width < 1 && rect.height < 1) return false;
    var style = window.getComputedStyle(node);
    return (
      style.visibility !== "hidden" &&
      style.display !== "none" &&
      style.opacity !== "0"
    );
  }

  function roleOf(node) {
    var explicit = node.getAttribute("role");
    if (explicit) return explicit;
    var tag = node.tagName.toLowerCase();
    if (tag === "a") return node.hasAttribute("href") ? "link" : "generic";
    if (tag === "button" || tag === "summary") return "button";
    if (tag === "select") return node.multiple ? "listbox" : "combobox";
    if (tag === "textarea") return "textbox";
    if (tag === "input") {
      var type = (node.getAttribute("type") || "text").toLowerCase();
      if (type === "checkbox" || type === "radio" || type === "range") return type;
      if (type === "submit" || type === "button" || type === "reset") return "button";
      if (type === "search") return "searchbox";
      return "textbox";
    }
    if (tag === "img") return "img";
    return "generic";
  }

  /**
   * A coarse ARIA name: label element, aria-label, aria-labelledby, placeholder,
   * alt, title, then the element's own text. Good enough to click by, and it
   * never reads a password's value.
   */
  function labelOf(node) {
    if (!node || !node.tagName) return "";
    var label =
      node.getAttribute("aria-label") ||
      (node.getAttribute("aria-labelledby") &&
        textOfLabels(node.getAttribute("aria-labelledby"))) ||
      (node.labels && node.labels.length ? clip(node.labels[0].textContent, 80) : "") ||
      node.getAttribute("placeholder") ||
      node.getAttribute("alt") ||
      node.getAttribute("title") ||
      node.getAttribute("name") ||
      node.textContent ||
      "";
    return clip(String(label).replace(/\s+/g, " ").trim(), 90);
  }

  function textOfLabels(ids) {
    var parts = [];
    String(ids)
      .split(/\s+/)
      .forEach(function (id) {
        var node = document.getElementById(id);
        if (node) parts.push(node.textContent);
      });
    return parts.join(" ");
  }

  /** Never returns a password's value: it reports "filled" and nothing else. */
  function valueOf(node) {
    if (!node) return null;
    if (node.tagName === "INPUT" && (node.getAttribute("type") || "").toLowerCase() === "password") {
      return node.value ? "<set>" : "";
    }
    if ("value" in node && node.value != null) return String(node.value);
    if (node.isContentEditable) return String(node.textContent || "");
    return null;
  }

  function stateOf(node) {
    if (node.tagName === "INPUT") {
      var type = (node.getAttribute("type") || "").toLowerCase();
      if (type === "checkbox" || type === "radio") return node.checked ? "checked" : "unchecked";
    }
    if (node.getAttribute("aria-checked")) return "aria-checked=" + node.getAttribute("aria-checked");
    if (node.disabled) return "disabled";
    if (node.tagName === "A" && node.href) return "href=" + node.href;
    return null;
  }

  function selectorFor(node) {
    if (!node || !node.tagName) return "";
    if (node.id) return "#" + node.id;
    var parts = [];
    var walk = node;
    while (walk && walk.tagName && parts.length < 4) {
      var part = walk.tagName.toLowerCase();
      if (walk.name) part += '[name="' + walk.name + '"]';
      else if (walk.className && typeof walk.className === "string") {
        var first = walk.className.trim().split(/\s+/)[0];
        if (first) part += "." + first;
      }
      parts.unshift(part);
      walk = walk.parentElement;
    }
    return parts.join(" > ");
  }

  /** Resolves a target: `[7]` from the last snapshot, or a CSS selector. */
  var byIndex = Object.create(null);

  function resolve(target) {
    if (target == null) return null;
    var text = String(target).trim();
    var match = text.match(/^\[(\d+)\]$/);
    if (match) {
      var node = byIndex[match[1]];
      if (!node || !node.isConnected) {
        throw new Error(
          "[" + match[1] + "] is no longer on the page. Take a fresh browser_snapshot.",
        );
      }
      return node;
    }
    if (text === "active") return document.activeElement;
    var found = document.querySelector(text);
    if (!found) throw new Error('no element matches "' + text + '"');
    return found;
  }

  function rectOf(node) {
    var rect = node.getBoundingClientRect();
    return [
      Math.round(rect.left),
      Math.round(rect.top),
      Math.round(rect.width),
      Math.round(rect.height),
    ];
  }

  /* ------------------------------------------------------------- snapshot */

  function snapshot(args) {
    byIndex = Object.create(null);
    var interactiveOnly = args.interactive_only !== false;
    var maxNodes = Math.min(Math.max(args.max_nodes || 400, 1), 2000);
    var scope = args.selector ? document.querySelector(args.selector) : document.body;
    if (!scope) throw new Error('no element matches "' + args.selector + '"');

    var nodes = scope.querySelectorAll(ACTIONABLE);
    var lines = [];
    var index = 0;
    var belowFold = 0;

    for (var i = 0; i < nodes.length && index < maxNodes; i += 1) {
      var node = nodes[i];
      if (!visible(node)) continue;
      var role = roleOf(node);
      if (interactiveOnly && role === "generic" && !node.hasAttribute("tabindex")) continue;

      var rect = node.getBoundingClientRect();
      if (rect.top > window.innerHeight || rect.bottom < 0) {
        belowFold += 1;
        if (!args.all) continue;
      }

      index += 1;
      byIndex[String(index)] = node;
      var line = "[" + index + "]  " + role + '  "' + labelOf(node) + '"';
      var value = valueOf(node);
      if (value !== null && value !== "") {
        line += '  value "' + clip(value, 60) + '"';
      }
      var state = stateOf(node);
      if (state) line += "  " + state;
      line += "  rect " + rectOf(node).join(",");
      lines.push(line);
    }

    var body = {
      url: location.href,
      title: document.title,
      viewport: [window.innerWidth, window.innerHeight],
      scroll: [Math.round(window.scrollX), Math.round(window.scrollY)],
      scrollHeight: document.documentElement.scrollHeight,
      ready: document.readyState,
      frame: location.href,
      frameCount: window.__loomFrameCount || 1,
      nodes: lines,
      truncated: index >= maxNodes,
      belowFold: belowFold,
      userActivity: userActivity(args.since_ms),
      dialogs: dialogs.slice(-5),
    };
    return body;
  }

  /* ----------------------------------------------------------------- read */

  function read(args) {
    var scope = args.selector ? document.querySelector(args.selector) : bestRoot();
    if (!scope) throw new Error("nothing readable on this page");
    var limit = Math.min(Math.max(args.max_chars || 6000, 200), 40000);

    var text = (scope.innerText || scope.textContent || "")
      .replace(/[ \t\u00a0]+/g, " ")
      .replace(/\n{3,}/g, "\n\n")
      .trim();

    // One pass of the actionable nodes, inlined as `[n]` markers, so the model
    // can read a page and then click exactly what it read.
    byIndex = Object.create(null);
    var nodes = scope.querySelectorAll(ACTIONABLE);
    var index = 0;
    var marks = [];
    for (var i = 0; i < nodes.length && index < 120; i += 1) {
      var node = nodes[i];
      if (!visible(node)) continue;
      index += 1;
      byIndex[String(index)] = node;
      var label = labelOf(node);
      if (label) marks.push("[" + index + "] " + roleOf(node) + ' "' + label + '"');
    }

    var truncated = false;
    if (text.length > limit) {
      text = text.slice(0, limit) + "\n… [truncated]";
      truncated = true;
    }

    return {
      url: location.href,
      title: document.title,
      text: text,
      truncated: truncated,
      marks: marks,
      userActivity: userActivity(args.since_ms),
    };
  }

  function bestRoot() {
    var candidates = ["main", "article", '[role="main"]', "#content", "#main", "body"];
    var best = document.body;
    var bestLength = 0;
    candidates.forEach(function (selector) {
      var node = document.querySelector(selector);
      if (!node) return;
      var length = (node.innerText || "").length;
      if (length > bestLength) {
        bestLength = length;
        best = node;
      }
    });
    return best;
  }

  function userActivity(sinceMs) {
    var since = sinceMs ? now() - sinceMs : now() - USER_WINDOW_MS;
    var recent = userEvents.filter(function (event) {
      return event.at >= since;
    });
    var typed = [];
    Object.keys(userTyped).forEach(function (selector) {
      if (userTyped[selector] >= since) typed.push(selector);
    });
    return {
      lastInputAt: lastUserAt,
      idleMs: lastUserAt ? now() - lastUserAt : null,
      recent: recent.slice(-12),
      typedFields: typed,
    };
  }

  /* ------------------------------------------------------------------ act */

  /**
   * Dispatches a full pointer sequence at a point.
   *
   * `isTrusted` is false for all of these, which a handful of sites check. That
   * is the documented cost of doing it without CDP `Input.*`; the escape hatch
   * for the sites that care is `browser_evaluate`.
   */
  function pointer(node, kind) {
    var rect = node.getBoundingClientRect();
    var x = rect.left + rect.width / 2;
    var y = rect.top + rect.height / 2;
    var base = {
      bubbles: true,
      cancelable: true,
      composed: true,
      clientX: x,
      clientY: y,
      view: window,
      button: 0,
    };
    function fire(type, extra) {
      var init = base;
      for (var key in extra || {}) init[key] = extra[key];
      node.dispatchEvent(new MouseEvent(type, init));
    }
    injecting = true;
    try {
      if (kind === "hover") {
        fire("mouseover");
        fire("mouseenter");
        fire("mousemove");
        return;
      }
      fire("mouseover");
      fire("mouseenter");
      fire("mousemove");
      fire("mousedown", { buttons: 1 });
      if (node.focus) {
        try {
          node.focus({ preventScroll: true });
        } catch (error) {
          node.focus();
        }
      }
      fire("mouseup", { buttons: 0 });
      fire("click");
      if (kind === "dblclick") {
        fire("mousedown", { buttons: 1 });
        fire("mouseup", { buttons: 0 });
        fire("click");
        fire("dblclick");
      }
    } finally {
      injecting = false;
    }
  }

  function click(args) {
    var node = resolve(args.target);
    if (!node) throw new Error("browser_click needs a target");
    if (node.disabled) throw new Error('"' + labelOf(node) + '" is disabled');
    var kind = args.kind || "click";
    if (kind === "down" || kind === "up") {
      injecting = true;
      try {
        node.dispatchEvent(
          new MouseEvent(kind === "down" ? "mousedown" : "mouseup", {
            bubbles: true,
            cancelable: true,
            buttons: kind === "down" ? 1 : 0,
            view: window,
          }),
        );
      } finally {
        injecting = false;
      }
    } else {
      pointer(node, kind === "dblclick" ? "dblclick" : "click");
    }
    return {
      url: location.href,
      title: document.title,
      target: describe(node),
      label: labelOf(node),
      userActivity: userActivity(4000),
    };
  }

  function setValue(node, text) {
    injecting = true;
    try {
      if (node.isContentEditable) {
        node.textContent = text;
        node.dispatchEvent(new InputEvent("input", { bubbles: true }));
        return;
      }
      var proto =
        node.tagName === "TEXTAREA"
          ? window.HTMLTextAreaElement.prototype
          : window.HTMLInputElement.prototype;
      var setter = Object.getOwnPropertyDescriptor(proto, "value");
      if (setter && setter.set) setter.set.call(node, text);
      else node.value = text;
      node.dispatchEvent(new Event("input", { bubbles: true }));
      node.dispatchEvent(new Event("change", { bubbles: true }));
    } finally {
      injecting = false;
    }
  }

  /** The one refusal: never overwrite a password the model did not type. */
  function guardPassword(node, text) {
    if (!node || node.tagName !== "INPUT") return;
    var type = (node.getAttribute("type") || "").toLowerCase();
    if (type !== "password") return;
    if (!node.value) return;
    var typed = userTyped[selectorFor(node)];
    if (typed) {
      throw new Error(
        "refused: this password field already holds a value the user typed. " +
          "It was left untouched. Ask the user, or use a field you filled yourself.",
      );
    }
  }

  function type(args) {
    var node = resolve(args.target);
    if (!node) throw new Error("browser_type needs a target");
    var text = args.text == null ? "" : String(args.text);
    guardPassword(node, text);

    if (args.clear !== false) setValue(node, "");
    if (args.clear === false && node.value) {
      // Append mode: keep what is there and add to it.
      setValue(node, node.value + text);
    } else {
      setValue(node, text);
    }
    return {
      target: describe(node),
      label: labelOf(node),
      value: valueOf(node),
      userActivity: userActivity(4000),
    };
  }

  var KEYS = {
    enter: { key: "Enter", code: "Enter", keyCode: 13 },
    tab: { key: "Tab", code: "Tab", keyCode: 9 },
    escape: { key: "Escape", code: "Escape", keyCode: 27 },
    backspace: { key: "Backspace", code: "Backspace", keyCode: 8 },
    delete: { key: "Delete", code: "Delete", keyCode: 46 },
    arrowup: { key: "ArrowUp", code: "ArrowUp", keyCode: 38 },
    arrowdown: { key: "ArrowDown", code: "ArrowDown", keyCode: 40 },
    arrowleft: { key: "ArrowLeft", code: "ArrowLeft", keyCode: 37 },
    arrowright: { key: "ArrowRight", code: "ArrowRight", keyCode: 39 },
    space: { key: " ", code: "Space", keyCode: 32 },
  };

  function press(args) {
    var spec = KEYS[String(args.key || "").toLowerCase()];
    var target = args.target ? resolve(args.target) : document.activeElement || document.body;
    var repeat = Math.min(Math.max(args.repeat || 1, 1), 50);

    if (!spec) {
      // A single character: type it where the focus is.
      var text = String(args.key || "");
      if (text.length === 1 && target && "value" in target) {
        setValue(target, (target.value || "") + text);
        return { target: describe(target), typed: text };
      }
      throw new Error('unknown key "' + args.key + '"');
    }

    injecting = true;
    try {
      for (var i = 0; i < repeat; i += 1) {
        ["keydown", "keypress", "keyup"].forEach(function (type) {
          var event = new KeyboardEvent(type, {
            key: spec.key,
            code: spec.code,
            keyCode: spec.keyCode,
            which: spec.keyCode,
            bubbles: true,
            cancelable: true,
            view: window,
          });
          target.dispatchEvent(event);
        });
        if (spec.key === "Enter" && target && target.form) {
          if (typeof target.form.requestSubmit === "function") target.form.requestSubmit();
          else target.form.submit();
        }
      }
    } finally {
      injecting = false;
    }
    return { key: spec.key, target: describe(target) };
  }

  function select(args) {
    var node = resolve(args.target);
    if (!node || node.tagName !== "SELECT") throw new Error("browser_select needs a <select>");
    var wanted = String(args.value == null ? args.label || "" : args.value).toLowerCase();
    var matched = null;
    for (var i = 0; i < node.options.length; i += 1) {
      var option = node.options[i];
      if (
        String(option.value).toLowerCase() === wanted ||
        String(option.textContent).trim().toLowerCase() === wanted
      ) {
        matched = option;
        break;
      }
    }
    if (!matched) {
      throw new Error(
        "no option matches \"" +
          args.value +
          "\". Options: " +
          Array.prototype.map
            .call(node.options, function (o) {
              return o.textContent.trim();
            })
            .slice(0, 20)
            .join(", "),
      );
    }
    injecting = true;
    try {
      node.value = matched.value;
      node.dispatchEvent(new Event("input", { bubbles: true }));
      node.dispatchEvent(new Event("change", { bubbles: true }));
    } finally {
      injecting = false;
    }
    return { label: labelOf(node), value: matched.value, text: matched.textContent.trim() };
  }

  function check(args) {
    var node = resolve(args.target);
    if (!node) throw new Error("browser_check needs a target");
    var wanted = args.checked !== false;
    if (node.checked !== wanted) pointer(node, "click");
    return {
      label: labelOf(node),
      checked: node.checked == null ? null : node.checked,
      userActivity: userActivity(4000),
    };
  }

  function scroll(args) {
    var node = args.target ? resolve(args.target) : null;
    var amount = Math.min(Math.max(args.amount || 600, -20000), 20000);
    if (node) node.scrollBy({ top: amount, behavior: "instant" });
    else window.scrollBy({ top: amount, behavior: "instant" });
    return {
      scroll: [Math.round(window.scrollX), Math.round(window.scrollY)],
      scrollHeight: document.documentElement.scrollHeight,
      viewport: [window.innerHeight, window.innerWidth],
    };
  }

  function fillForm(args) {
    var fields = args.fields || [];
    var filled = [];
    var refused = [];
    for (var i = 0; i < fields.length; i += 1) {
      var field = fields[i];
      try {
        var node = resolve(field.target);
        if (!node) {
          refused.push({ target: field.target, reason: "not found" });
          continue;
        }
        if (field.value != null) {
          guardPassword(node, String(field.value));
          setValue(node, String(field.value));
          filled.push({ label: labelOf(node), value: valueOf(node) });
        }
        if (field.checked != null) {
          if (node.checked !== field.checked) pointer(node, "click");
          filled.push({ label: labelOf(node), checked: node.checked });
        }
      } catch (error) {
        refused.push({ target: field.target, reason: String(error.message || error) });
      }
    }
    return { filled: filled, refused: refused, userActivity: userActivity(4000) };
  }

  function drag(args) {
    var from = resolve(args.from);
    var to = resolve(args.to);
    if (!from || !to) throw new Error("browser_drag needs from and to");
    var a = from.getBoundingClientRect();
    var b = to.getBoundingClientRect();
    var start = { x: a.left + a.width / 2, y: a.top + a.height / 2 };
    var end = { x: b.left + b.width / 2, y: b.top + b.height / 2 };
    injecting = true;
    try {
      from.dispatchEvent(
        new MouseEvent("mousedown", { bubbles: true, clientX: start.x, clientY: start.y, view: window }),
      );
      to.dispatchEvent(
        new MouseEvent("mousemove", { bubbles: true, clientX: end.x, clientY: end.y, view: window }),
      );
      to.dispatchEvent(
        new MouseEvent("mouseup", { bubbles: true, clientX: end.x, clientY: end.y, view: window }),
      );
    } finally {
      injecting = false;
    }
    return { from: describe(from), to: describe(to) };
  }

  /* --------------------------------------------------------------- assert */

  function assert(args) {
    var kind = args.check;
    var value = args.value;
    var pass = false;
    var evidence = "";

    if (kind === "url_matches") {
      pass = new RegExp(value).test(location.href);
      evidence = location.href;
    } else if (kind === "title_matches") {
      pass = new RegExp(value).test(document.title);
      evidence = document.title;
    } else if (kind === "text_present") {
      var haystack = document.body ? document.body.innerText || "" : "";
      pass = haystack.indexOf(value) !== -1;
      evidence = pass ? "found" : "not in " + haystack.length + " chars of text";
    } else if (kind === "text_absent") {
      var body = document.body ? document.body.innerText || "" : "";
      pass = body.indexOf(value) === -1;
      evidence = pass ? "absent" : "present";
    } else if (kind === "exists") {
      var node = document.querySelector(value);
      pass = Boolean(node) && visible(node);
      evidence = node ? "found " + describe(node) : "no match";
    } else if (kind === "not_exists") {
      pass = !document.querySelector(value);
      evidence = pass ? "absent" : "present";
    } else if (kind === "value_equals") {
      var field = resolve(args.target);
      var actual = valueOf(field);
      pass = String(actual) === String(value);
      evidence = "value " + JSON.stringify(actual);
    } else if (kind === "no_console_errors") {
      var errors = consoleLog.filter(function (entry) {
        return entry.level === "error";
      });
      pass = errors.length === 0;
      evidence = errors.length ? errors[errors.length - 1].text : "clean";
    } else if (kind === "ready") {
      pass = document.readyState === "complete";
      evidence = document.readyState;
    } else {
      throw new Error(
        'unknown check "' +
          kind +
          '"; use url_matches, title_matches, text_present, text_absent, exists, ' +
          "not_exists, value_equals, no_console_errors or ready",
      );
    }

    return { pass: pass, check: kind, value: value, evidence: evidence };
  }

  /* ----------------------------------------------------------------- page */

  function page() {
    return {
      url: location.href,
      title: document.title,
      ready: document.readyState,
      viewport: [window.innerWidth, window.innerHeight],
      scroll: [Math.round(window.scrollX), Math.round(window.scrollY)],
      scrollHeight: document.documentElement.scrollHeight,
      console: consoleLog.slice(-120),
      failures: failures.slice(-40),
      dialogs: dialogs.slice(-5),
      userActivity: userActivity(),
      counts: {
        console: consoleLog.length,
        failures: failures.length,
        dialogs: dialogs.length,
      },
    };
  }

  function find(args) {
    var needle = String(args.text || "");
    if (!needle) throw new Error("browser_find needs text");
    var text = document.body ? document.body.innerText || "" : "";
    var haystack = args.ignore_case === false ? text : text.toLowerCase();
    var target = args.ignore_case === false ? needle : needle.toLowerCase();
    var hits = [];
    var from = 0;
    while (hits.length < (args.max || 20)) {
      var at = haystack.indexOf(target, from);
      if (at === -1) break;
      hits.push(clip(text.slice(Math.max(0, at - 60), at + needle.length + 60), 160));
      from = at + target.length;
    }
    return { text: needle, count: hits.length, hits: hits };
  }

  /* -------------------------------------------------------------- evaluate */

  function evaluate(args) {
    var expression = String(args.expression || "");
    if (!expression) throw new Error("browser_evaluate needs an expression");
    // `Function` rather than `eval`, so the expression can use `return`.
    var result = new Function("return (" + expression + ")")();
    var type = typeof result;
    var serialised;
    try {
      serialised = JSON.stringify(result);
    } catch (error) {
      serialised = null;
    }
    return {
      type: type,
      value: serialised === undefined ? String(result) : serialised,
      truncated: serialised ? serialised.length > 8000 : false,
      preview: clip(serialised === null ? String(result) : serialised, 8000),
    };
  }

  /* ---------------------------------------------------------------- handle */

  var OPS = {
    snapshot: snapshot,
    read: read,
    click: click,
    type: type,
    press: press,
    select: select,
    check: check,
    hover: function (args) {
      var node = resolve(args.target);
      pointer(node, "hover");
      return { label: labelOf(node) };
    },
    scroll: scroll,
    fill_form: fillForm,
    drag: drag,
    assert: assert,
    find: find,
    evaluate: evaluate,
    page: page,
    dialogs: function () {
      var out = dialogs.slice();
      if (args_drain) dialogs.length = 0;
      return { dialogs: out };
    },
    console: function () {
      return { console: consoleLog.slice(-200), failures: failures.slice(-40) };
    },
    reset: function () {
      userEvents.length = 0;
      userTyped = Object.create(null);
      lastUserAt = 0;
      return { ok: true };
    },
  };

  // `dialogs` drains on request so the same dialog is not answered twice.
  var args_drain = true;

  window.__loomBrowser = {
    /** Returns a JSON **string**: the only shape that survives every bridge. */
    handle: function (op, argsJson) {
      try {
        var fn = OPS[op];
        if (!fn) {
          return JSON.stringify({ error: "the collector does not implement " + op });
        }
        var args = argsJson ? JSON.parse(argsJson) : {};
        if (op === "dialogs") {
          var drained = dialogs.slice();
          dialogs.length = 0;
          return JSON.stringify({ dialogs: drained });
        }
        return JSON.stringify(fn(args));
      } catch (error) {
        return JSON.stringify({
          error: String((error && error.message) || error),
          stack: error && error.stack ? clip(error.stack, 400) : null,
        });
      }
    },
    /** Lets the host check liveness without going through a tool. */
    ping: function () {
      return JSON.stringify({ ok: true, version: 1 });
    },
  };

  // Frame count, so a snapshot can say how many frames it is not showing.
  try {
    if (window.top === window) {
      window.__loomFrameCount = window.frames.length;
    }
  } catch (error) {
    // Cross-origin: not our business.
  }
})();
