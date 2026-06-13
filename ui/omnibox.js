(function () {
  "use strict";

  const SECURE = "\u{1F512}";
  const INSECURE = "\u26A0\uFE0F";

  const URL_RE = /^(https?:\/\/)?([\w-]+\.)+[\w-]+(:[0-9]+)?(\/.*)?$/i;
  const IP_RE = /^(\d{1,3}\.){3}\d{1,3}(:\d+)?(\/.*)?$/;
  const HAS_SCHEME_RE = /^[a-z][a-z0-9+\-.]*:\/\//i;
  const SEARCH_ENGINES = {
    duckduckgo: "https://duckduckgo.com/?q={query}",
    google: "https://www.google.com/search?q={query}",
    bing: "https://www.bing.com/search?q={query}",
    brave: "https://search.brave.com/search?q={query}",
  };

  let _currentEngine = "duckduckgo";
  let _suggestions = [];
  let _autocompleteIndex = -1;

  function getOmniboxInput() {
    return document.getElementById("omnibox-input");
  }

  function getDropdown() {
    return document.getElementById("omnibox-dropdown");
  }

  function getSecurityIndicator() {
    return document.getElementById("security-indicator");
  }

  export function isUrl(input) {
    if (typeof input !== "string") return false;
    const trimmed = input.trim();
    if (trimmed.length === 0) return false;

    if (IP_RE.test(trimmed)) return true;

    if (HAS_SCHEME_RE.test(trimmed)) {
      try {
        new URL(trimmed);
        return true;
      } catch (_) {
        return false;
      }
    }

    if (URL_RE.test(trimmed)) return true;

    return false;
  }

  export function buildSearchUrl(query, engine) {
    const template = SEARCH_ENGINES[engine] || SEARCH_ENGINES.duckduckgo;
    return template.replace("{query}", encodeURIComponent(query));
  }

  export function normalizeUrl(input) {
    const trimmed = input.trim();
    if (!HAS_SCHEME_RE.test(trimmed)) {
      return "https://" + trimmed;
    }
    return trimmed;
  }

  export function setSearchEngine(engine) {
    if (SEARCH_ENGINES[engine]) {
      _currentEngine = engine;
    }
  }

  export function getSearchEngine() {
    return _currentEngine;
  }

  export function setSecurity(secure) {
    const el = getSecurityIndicator();
    if (!el) return;
    el.textContent = secure ? SECURE : INSECURE;
    el.title = secure ? "Connection is secure" : "Connection is not secure";
  }

  export function processInput(input) {
    if (isUrl(input)) {
      const url = normalizeUrl(input);
      return { type: "url", value: url };
    } else {
      const searchUrl = buildSearchUrl(input, _currentEngine);
      return { type: "search", value: searchUrl, query: input.trim() };
    }
  }

  export function getSuggestions(query) {
    if (!query || query.trim().length === 0) return [];
    const trimmed = query.trim().toLowerCase();
    return _suggestions.filter(function (s) {
      return s.toLowerCase().indexOf(trimmed) !== -1;
    });
  }

  export function setSuggestions(suggestions) {
    _suggestions = suggestions.slice();
  }

  export function renderDropdown(matches) {
    const dropdown = getDropdown();
    if (!dropdown) return;

    if (!matches || matches.length === 0) {
      dropdown.hidden = true;
      return;
    }

    dropdown.innerHTML = "";
    matches.forEach(function (match, i) {
      const item = document.createElement("button");
      item.className = "dropdown-item" + (i === _autocompleteIndex ? " selected" : "");
      item.role = "option";
      item.textContent = match;
      item.addEventListener("mousedown", function (e) {
        e.preventDefault();
        const input = getOmniboxInput();
        if (input) {
          input.value = match;
          dropdown.hidden = true;
          input.focus();
        }
      });
      dropdown.appendChild(item);
    });
    dropdown.hidden = false;
  }

  export function initOmnibox(options) {
    if (options && options.searchEngine) {
      _currentEngine = options.searchEngine;
    }

    const input = getOmniboxInput();
    const dropdown = getDropdown();

    if (!input || !dropdown) return;

    input.addEventListener("input", function () {
      _autocompleteIndex = -1;
      const matches = getSuggestions(input.value);
      renderDropdown(matches);
    });

    input.addEventListener("keydown", function (e) {
      const items = dropdown.querySelectorAll(".dropdown-item");

      if (e.key === "ArrowDown") {
        e.preventDefault();
        _autocompleteIndex = Math.min(_autocompleteIndex + 1, items.length - 1);
        renderDropdown(
          Array.from(items).map(function (el) {
            return el.textContent;
          })
        );
        return;
      }

      if (e.key === "ArrowUp") {
        e.preventDefault();
        _autocompleteIndex = Math.max(_autocompleteIndex - 1, -1);
        renderDropdown(
          Array.from(items).map(function (el) {
            return el.textContent;
          })
        );
        return;
      }

      if (e.key === "Enter") {
        e.preventDefault();
        dropdown.hidden = true;
        var result = processInput(input.value);
        navigateTo(result.value);
        return;
      }

      if (e.key === "Escape") {
        dropdown.hidden = true;
        input.blur();
        return;
      }
    });

    input.addEventListener("blur", function () {
      setTimeout(function () {
        dropdown.hidden = true;
      }, 200);
    });

    input.addEventListener("focus", function () {
      if (input.value) {
        var matches = getSuggestions(input.value);
        renderDropdown(matches);
      }
    });
  }

  function navigateTo(url) {
    var event = new CustomEvent("kore-navigate", {
      detail: { url: url },
      bubbles: true,
    });
    document.dispatchEvent(event);
  }

  if (typeof window !== "undefined") {
    window.koreOmnibox = {
      isUrl: isUrl,
      buildSearchUrl: buildSearchUrl,
      normalizeUrl: normalizeUrl,
      setSearchEngine: setSearchEngine,
      getSearchEngine: getSearchEngine,
      setSecurity: setSecurity,
      processInput: processInput,
      getSuggestions: getSuggestions,
      setSuggestions: setSuggestions,
      renderDropdown: renderDropdown,
      initOmnibox: initOmnibox,
    };
  }
})();
