// Runs before the module, once per isolate.
// deno_core's console prints to stdout, which carries the protocol: replace it with one that
// goes to the host (stderr under serve, the trace under zoo-module test). Then hide deno_core's
// op table so module code reaches the host only through the APIs in web.js and `zoo`.
((core) => {
  const { op_zoo_log } = core.ops;
  const format = (args) =>
    args.map((arg) => {
      if (typeof arg === "string") return arg;
      try {
        return JSON.stringify(arg) ?? String(arg);
      } catch {
        return String(arg);
      }
    }).join(" ");
  const log = {};
  for (const level of ["debug", "info", "warn", "error"]) {
    log[level] = (...args) => op_zoo_log(level, format(args));
  }
  globalThis.console = { ...log, log: log.info };

  const { op_zoo_credential, op_zoo_wait, op_zoo_captcha } = core.ops;
  // `retryAfter`: the site refuses for that many seconds (a free download limit, a used-up
  // quota); the app waits it out without holding a download slot, then tries again.
  const fail = (message, { hint, retryAfter } = {}) => {
    const error = new Error(message);
    if (hint !== undefined) error.hint = String(hint);
    if (retryAfter !== undefined) {
      const seconds = Math.ceil(Number(retryAfter));
      if (!(seconds > 0)) throw new TypeError("zoo.fail: retryAfter must be a positive number of seconds");
      error.retryAfter = seconds;
    }
    return error;
  };
  const solved = (solution) => {
    if (solution === null) throw fail("captcha was not solved", { hint: "the user declined or did not answer in time" });
    return solution;
  };

  const zoo = Object.freeze({
    log: Object.freeze({ ...log }),
    fail,
    credentials: Object.freeze({
      has: (key) => op_zoo_credential(String(key)) !== null,
      // An opaque stand-in for the value: put it in a fetch url, header or body and the host
      // sends the real value. Module code never sees credential values.
      placeholder(key) {
        const placeholder = op_zoo_credential(String(key));
        if (placeholder === null) {
          throw fail(`credential '${key}' is not available`, {
            hint: "declare it in manifest.json permissions.credentials; check zoo.credentials.has(key) first",
          });
        }
        return placeholder;
      },
    }),
    captcha: Object.freeze({
      recaptcha: async ({ siteKey, pageUrl }) =>
        solved(await op_zoo_captcha({ kind: "recaptcha", siteKey: String(siteKey), pageUrl: String(pageUrl) })),
      image: async (imageUrl) => solved(await op_zoo_captcha({ kind: "image", url: String(imageUrl) })),
    }),
    // Something only the user knows (an extraction code). With `remember` the app answers again
    // for this download without asking; `error` (the answer did not work) always asks.
    ask: async ({ id, message, secret = false, remember = false, error } = {}) => {
      const text = (value) => (value !== null && typeof value === "object" ? value : String(value));
      const answer = await op_zoo_captcha({
        kind: "ask",
        id: String(id),
        message: text(message),
        secret: Boolean(secret),
        remember: Boolean(remember),
        error: error === undefined || error === null ? null : text(error),
      });
      if (answer === null) throw fail("the user did not answer", { hint: "they declined or did not answer in time" });
      return answer;
    },
    // A site countdown. Shown to the user; does not count against the time limit.
    wait: async (seconds, reason) => {
      const ms = Math.round(Number(seconds) * 1000);
      if (!(ms >= 0)) throw new TypeError("zoo.wait: seconds must be a non-negative number");
      await op_zoo_wait(ms, reason === undefined || reason === null ? "" : String(reason));
    },
  });

  // Called by the generated entry module with the module's default export.
  // `work` is "resolve" or "list"; `url` null means load and shape check only ({"checked": true}).
  // Otherwise settles with a JSON string: {"ok": result} | {"err": {message, hint, stack, retryAfter}} |
  // {"invalid": why}; a list call on a module without `list` gives {"ok": null}.
  const run = async (module, work, url) => {
    if (module === null || typeof module !== "object" || typeof module.resolve !== "function") {
      return JSON.stringify({ invalid: "the default export has no resolve function" });
    }
    if (module.list !== undefined && typeof module.list !== "function") {
      return JSON.stringify({ invalid: "the default export's list is not a function" });
    }
    if (url === null) return JSON.stringify({ checked: true });
    if (work === "list" && module.list === undefined) return JSON.stringify({ ok: null });
    try {
      const result = await (work === "list" ? module.list(url, zoo) : module.resolve(url, zoo));
      return JSON.stringify({ ok: result ?? null });
    } catch (e) {
      const isObject = e !== null && typeof e === "object";
      return JSON.stringify({
        err: {
          message: isObject && "message" in e ? String(e.message) : String(e),
          hint: isObject && typeof e.hint === "string" ? e.hint : null,
          stack: isObject && typeof e.stack === "string" ? e.stack : null,
          retryAfter: isObject && typeof e.retryAfter === "number" ? e.retryAfter : null,
        },
      });
    }
  };
  Object.defineProperty(globalThis, "__zooRun", { value: run });
  // deno_core exposes its op table both as Deno.core and as __bootstrap.core.
  delete globalThis.Deno;
  delete globalThis.__bootstrap;
  // core-js keeps its state in closures; the global is only for sharing between copies.
  delete globalThis["__core-js_shared__"];
})(Deno.core);
