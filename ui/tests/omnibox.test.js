(function () {
  "use strict";

  var module = window.koreOmnibox;
  var tests = [];

  function assert(cond, msg) {
    if (!cond) throw new Error(msg || "assertion failed");
  }

  tests.push({
    name: "isUrl returns false for empty string",
    run: function () {
      assert(module.isUrl("") === false, "empty string is not a URL");
      assert(module.isUrl("   ") === false, "whitespace is not a URL");
    },
  });

  tests.push({
    name: "isUrl detects standard URLs",
    run: function () {
      assert(module.isUrl("https://example.com"), "https URL");
      assert(module.isUrl("http://example.com"), "http URL");
      assert(module.isUrl("https://sub.example.com/page"), "URL with path");
      assert(module.isUrl("https://example.com:8080"), "URL with port");
    },
  });

  tests.push({
    name: "isUrl detects URLs without scheme",
    run: function () {
      assert(module.isUrl("example.com"), "domain without scheme");
      assert(module.isUrl("sub.example.com"), "subdomain without scheme");
      assert(module.isUrl("example.com/page"), "domain with path without scheme");
    },
  });

  tests.push({
    name: "isUrl rejects search queries",
    run: function () {
      assert(module.isUrl("hello world") === false, "multi-word query");
      assert(module.isUrl("my search query") === false, "query with spaces");
    },
  });

  tests.push({
    name: "isUrl detects IP addresses",
    run: function () {
      assert(module.isUrl("192.168.1.1"), "IPv4 address");
      assert(module.isUrl("10.0.0.1:8000"), "IPv4 with port");
    },
  });

  tests.push({
    name: "normalizeUrl adds https scheme when missing",
    run: function () {
      assert(
        module.normalizeUrl("example.com") === "https://example.com",
        "adds https"
      );
      assert(
        module.normalizeUrl("https://example.com") === "https://example.com",
        "preserves existing https"
      );
    },
  });

  tests.push({
    name: "buildSearchUrl constructs correct URL",
    run: function () {
      var url = module.buildSearchUrl("hello world", "duckduckgo");
      assert(url.indexOf("q=hello+world") !== -1, "duckduckgo search");
      var google = module.buildSearchUrl("test", "google");
      assert(google.indexOf("q=test") !== -1, "google search");
    },
  });

  tests.push({
    name: "processInput distinguishes URL from search",
    run: function () {
      var urlResult = module.processInput("https://example.com");
      assert(urlResult.type === "url", "URL input returns URL type");
      assert(
        urlResult.value === "https://example.com",
        "URL input returns URL value"
      );

      var searchResult = module.processInput("hello world");
      assert(searchResult.type === "search", "search input returns search type");
      assert(
        searchResult.query === "hello world",
        "search input preserves query"
      );
    },
  });

  tests.push({
    name: "setSecurity updates indicator",
    run: function () {
      var container = document.createElement("div");
      container.id = "security-indicator";
      document.body.appendChild(container);

      module.setSecurity(true);
      assert(container.textContent === "\u{1F512}", "secure indicator");

      module.setSecurity(false);
      assert(container.textContent === "\u26A0\uFE0F", "insecure indicator");

      document.body.removeChild(container);
    },
  });

  return {
    name: "Omnibox Tests",
    tests: tests,
  };
})();
