(function () {
  "use strict";

  var module = window.koreTabs;
  var tests = [];

  function assert(cond, msg) {
    if (!cond) throw new Error(msg || "assertion failed");
  }

  function setup() {
    module.reset();
  }

  tests.push({
    name: "createTab increments tab count",
    run: function () {
      setup();
      assert(module.getTabCount() === 0, "starts empty");
      module.createTab("Tab 1");
      assert(module.getTabCount() === 1, "one tab after create");
      module.createTab("Tab 2");
      assert(module.getTabCount() === 2, "two tabs after create");
    },
  });

  tests.push({
    name: "createTab sets active tab",
    run: function () {
      setup();
      var tab = module.createTab("Active Tab");
      assert(tab !== null, "tab exists");
      assert(module.getActiveTabId() === tab.id, "active id matches");
      assert(module.getActiveTab().title === "Active Tab", "active tab title");
    },
  });

  tests.push({
    name: "closeTab removes tab and switches active",
    run: function () {
      setup();
      var t1 = module.createTab("Tab 1");
      var t2 = module.createTab("Tab 2");
      assert(module.getTabCount() === 2, "two tabs");

      module.closeTab(t2.id);
      assert(module.getTabCount() === 1, "one tab after close");
      assert(module.getActiveTabId() === t1.id, "active falls back to first");
    },
  });

  tests.push({
    name: "closeTab returns false for missing id",
    run: function () {
      setup();
      assert(module.closeTab(999) === false, "returns false for missing tab");
    },
  });

  tests.push({
    name: "switchTab changes active tab",
    run: function () {
      setup();
      var t1 = module.createTab("Tab 1");
      var t2 = module.createTab("Tab 2");
      assert(module.getActiveTabId() === t2.id, "last created is active");

      module.switchTab(t1.id);
      assert(module.getActiveTabId() === t1.id, "switched to tab 1");
    },
  });

  tests.push({
    name: "switchToNextTab cycles forward",
    run: function () {
      setup();
      var t1 = module.createTab("Tab 1");
      var t2 = module.createTab("Tab 2");
      var t3 = module.createTab("Tab 3");

      module.switchTab(t1.id);
      module.switchToNextTab();
      assert(module.getActiveTabId() === t2.id, "forward to tab 2");

      module.switchToNextTab();
      assert(module.getActiveTabId() === t3.id, "forward to tab 3");

      module.switchToNextTab();
      assert(module.getActiveTabId() === t1.id, "wraps to tab 1");
    },
  });

  tests.push({
    name: "switchToPrevTab cycles backward",
    run: function () {
      setup();
      var t1 = module.createTab("Tab 1");
      var t2 = module.createTab("Tab 2");
      var t3 = module.createTab("Tab 3");

      module.switchTab(t1.id);
      module.switchToPrevTab();
      assert(module.getActiveTabId() === t3.id, "backward wraps to tab 3");
    },
  });

  tests.push({
    name: "pinTab and unpinTab work correctly",
    run: function () {
      setup();
      var tab = module.createTab("Pinnable");
      assert(tab.isPinned === false, "starts unpinned");

      module.pinTab(tab.id);
      assert(module.getTab(tab.id).isPinned === true, "pinned after pinTab");

      var pinned = module.getPinnedTabs();
      assert(pinned.length === 1, "one pinned tab");

      module.unpinTab(tab.id);
      assert(module.getTab(tab.id).isPinned === false, "unpinned after unpinTab");
    },
  });

  tests.push({
    name: "updateTab modifies title and url",
    run: function () {
      setup();
      var tab = module.createTab("Initial");
      module.updateTab(tab.id, { title: "Updated", url: "https://example.com" });
      assert(module.getTab(tab.id).title === "Updated", "title updated");
      assert(
        module.getTab(tab.id).url === "https://example.com",
        "url updated"
      );
    },
  });

  tests.push({
    name: "keyboard shortcut Ctrl+T creates tab",
    run: function () {
      setup();
      var event = new KeyboardEvent("keydown", {
        key: "t",
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      });
      document.dispatchEvent(event);
      assert(
        module.getTabCount() >= 1,
        "Ctrl+T creates a tab (or keeps default)"
      );
    },
  });

  tests.push({
    name: "keyboard shortcut Ctrl+W closes active tab",
    run: function () {
      setup();
      var t1 = module.createTab("Tab 1");
      module.createTab("Tab 2");
      assert(module.getTabCount() === 2, "two tabs before close");

      module.switchTab(t1.id);

      var event = new KeyboardEvent("keydown", {
        key: "w",
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      });
      document.dispatchEvent(event);
      assert(module.getTabCount() === 1, "one tab after Ctrl+W");
    },
  });

  return {
    name: "Tabs Tests",
    tests: tests,
  };
})();
