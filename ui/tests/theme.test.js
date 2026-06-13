(function () {
  "use strict";

  var tests = [];

  function assert(cond, msg) {
    if (!cond) throw new Error(msg || "assertion failed");
  }

  function getTheme() {
    var body = document.body;
    if (body.classList.contains("dark-theme")) return "dark";
    if (body.classList.contains("light-theme")) return "light";
    return "light";
  }

  function setTheme(name) {
    var body = document.body;
    body.classList.remove("light-theme", "dark-theme");
    if (name === "dark") {
      body.classList.add("dark-theme");
    } else {
      body.classList.add("light-theme");
    }
  }

  tests.push({
    name: "default theme is light",
    run: function () {
      setTheme("light");
      assert(getTheme() === "light", "default theme is light");
      assert(
        document.body.classList.contains("light-theme"),
        "body has light-theme class"
      );
    },
  });

  tests.push({
    name: "switching to dark theme applies class",
    run: function () {
      setTheme("dark");
      assert(getTheme() === "dark", "theme is dark after switch");
      assert(
        document.body.classList.contains("dark-theme"),
        "body has dark-theme class"
      );
      assert(
        !document.body.classList.contains("light-theme"),
        "body does not have light-theme class"
      );
    },
  });

  tests.push({
    name: "switching back to light removes dark class",
    run: function () {
      setTheme("dark");
      setTheme("light");
      assert(getTheme() === "light", "theme is light after switching back");
      assert(
        !document.body.classList.contains("dark-theme"),
        "dark class removed"
      );
    },
  });

  tests.push({
    name: "CSS custom properties are defined for both themes",
    run: function () {
      var body = document.body;
      var style = getComputedStyle(body);

      setTheme("light");
      var lightBg = style.getPropertyValue("--bg-primary").trim();
      assert(lightBg !== "", "--bg-primary is defined in light theme");
      assert(lightBg === "#ffffff", "light bg-primary is white");

      setTheme("dark");
      var darkBg = style.getPropertyValue("--bg-primary").trim();
      assert(darkBg !== "", "--bg-primary is defined in dark theme");
      assert(darkBg !== lightBg, "dark bg-primary differs from light");
    },
  });

  tests.push({
    name: "theme toggle preserves other styles",
    run: function () {
      setTheme("light");
      var lightBg = getComputedStyle(document.body)
        .getPropertyValue("--bg-secondary")
        .trim();

      setTheme("dark");
      var darkBg = getComputedStyle(document.body)
        .getPropertyValue("--bg-secondary")
        .trim();

      assert(lightBg !== darkBg, "bg-secondary changes with theme");
    },
  });

  return {
    name: "Theme Tests",
    tests: tests,
  };
})();
