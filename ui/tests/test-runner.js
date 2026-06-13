(function () {
  "use strict";

  var results = [];

  window.koreTestRunner = {
    run: function (testModules) {
      results = [];
      var total = 0;
      var passed = 0;
      var failed = 0;

      testModules.forEach(function (module) {
        var suiteName = module.name || "Unnamed Suite";
        var suitePassed = 0;
        var suiteFailed = 0;
        var suiteResults = [];

        module.tests.forEach(function (test) {
          total++;
          try {
            test.run();
            passed++;
            suitePassed++;
            suiteResults.push({ name: test.name, status: "pass" });
          } catch (e) {
            failed++;
            suiteFailed++;
            suiteResults.push({
              name: test.name,
              status: "fail",
              error: e.message,
            });
          }
        });

        results.push({
          suite: suiteName,
          passed: suitePassed,
          failed: suiteFailed,
          tests: suiteResults,
        });
      });

      renderResults(total, passed, failed);
      return { total: total, passed: passed, failed: failed };
    },
  };

  function renderResults(total, passed, failed) {
    var container = document.getElementById("test-results");
    if (!container) return;

    var html = '<div class="test-summary">';
    html +=
      "<strong>" +
      total +
      " tests</strong> &mdash; " +
      '<span class="test-pass">' +
      passed +
      " passed</span>";
    if (failed > 0) {
      html +=
        ' &mdash; <span class="test-fail">' + failed + " failed</span>";
    }
    html += "</div>";

    results.forEach(function (suite) {
      html += '<div class="test-suite">';
      html +=
        '<h3 class="test-suite-title">' +
        escapeHtml(suite.suite) +
        " (" +
        suite.passed +
        "/" +
        (suite.passed + suite.failed) +
        ")</h3>";
      html += '<ul class="test-list">';
      suite.tests.forEach(function (test) {
        var cls = test.status === "pass" ? "test-pass" : "test-fail";
        var label = test.status === "pass" ? "PASS" : "FAIL";
        html += '<li class="test-item ' + cls + '">';
        html +=
          '<span class="test-status">' + label + "</span> " + escapeHtml(test.name);
        if (test.error) {
          html += '<div class="test-error">' + escapeHtml(test.error) + "</div>";
        }
        html += "</li>";
      });
      html += "</ul>";
      html += "</div>";
    });

    container.innerHTML = html;
  }

  function escapeHtml(str) {
    return String(str)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }
})();
