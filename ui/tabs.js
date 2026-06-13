(function () {
  "use strict";

  var _tabs = [];
  var _nextId = 1;
  var _activeId = null;
  var _pinnedIds = [];

  function Tab(id, title, url) {
    this.id = id;
    this.title = title || "New Tab";
    this.url = url || "about:newtab";
    this.isPinned = false;
    this.favicon = "";
  }

  export function createTab(title, url, options) {
    var tab = new Tab(_nextId++, title, url);
    if (options && options.isPinned) {
      tab.isPinned = true;
      _pinnedIds.push(tab.id);
    }
    _tabs.push(tab);
    activateTab(tab.id);
    return tab;
  }

  export function closeTab(id) {
    var idx = indexOf(id);
    if (idx === -1) return false;

    var wasActive = _activeId === id;
    _tabs.splice(idx, 1);

    var pinIdx = _pinnedIds.indexOf(id);
    if (pinIdx !== -1) {
      _pinnedIds.splice(pinIdx, 1);
    }

    if (wasActive) {
      if (_tabs.length > 0) {
        var newIdx = Math.min(idx, _tabs.length - 1);
        activateTab(_tabs[newIdx].id);
      } else {
        _activeId = null;
      }
    }

    return true;
  }

  export function switchTab(id) {
    if (indexOf(id) === -1) return false;
    activateTab(id);
    return true;
  }

  export function switchToNextTab() {
    if (_tabs.length === 0) return;
    var idx = indexOf(_activeId);
    var nextIdx = (idx + 1) % _tabs.length;
    activateTab(_tabs[nextIdx].id);
  }

  export function switchToPrevTab() {
    if (_tabs.length === 0) return;
    var idx = indexOf(_activeId);
    var prevIdx = (idx - 1 + _tabs.length) % _tabs.length;
    activateTab(_tabs[prevIdx].id);
  }

  export function pinTab(id) {
    var tab = getTab(id);
    if (!tab) return false;
    if (tab.isPinned) return true;
    tab.isPinned = true;
    if (_pinnedIds.indexOf(id) === -1) {
      _pinnedIds.push(id);
    }
    return true;
  }

  export function unpinTab(id) {
    var tab = getTab(id);
    if (!tab) return false;
    if (!tab.isPinned) return true;
    tab.isPinned = false;
    var pinIdx = _pinnedIds.indexOf(id);
    if (pinIdx !== -1) {
      _pinnedIds.splice(pinIdx, 1);
    }
    return true;
  }

  export function getTab(id) {
    return _tabs.find(function (t) {
      return t.id === id;
    }) || null;
  }

  export function getAllTabs() {
    return _tabs.slice();
  }

  export function getPinnedTabs() {
    return _tabs.filter(function (t) {
      return t.isPinned;
    });
  }

  export function getUnpinnedTabs() {
    return _tabs.filter(function (t) {
      return !t.isPinned;
    });
  }

  export function getActiveTab() {
    if (_activeId === null) return null;
    return getTab(_activeId);
  }

  export function getActiveTabId() {
    return _activeId;
  }

  export function getTabCount() {
    return _tabs.length;
  }

  export function updateTab(id, updates) {
    var tab = getTab(id);
    if (!tab) return false;
    if (updates.title !== undefined) tab.title = updates.title;
    if (updates.url !== undefined) tab.url = updates.url;
    if (updates.favicon !== undefined) tab.favicon = updates.favicon;
    return true;
  }

  export function reset() {
    _tabs = [];
    _nextId = 1;
    _activeId = null;
    _pinnedIds = [];
  }

  function indexOf(id) {
    return _tabs.findIndex(function (t) {
      return t.id === id;
    });
  }

  function activateTab(id) {
    _activeId = id;
  }

  function handleKeydown(e) {
    var isCtrl = e.ctrlKey || e.metaKey;

    if (isCtrl && e.key === "t") {
      e.preventDefault();
      createTab();
      return;
    }

    if (isCtrl && e.key === "w") {
      e.preventDefault();
      if (_activeId !== null) {
        closeTab(_activeId);
      }
      return;
    }

    if (isCtrl && e.key === "Tab") {
      e.preventDefault();
      if (e.shiftKey) {
        switchToPrevTab();
      } else {
        switchToNextTab();
      }
      return;
    }
  }

  export function initTabs() {
    if (typeof document !== "undefined") {
      document.addEventListener("keydown", handleKeydown);
    }
    if (_tabs.length === 0) {
      createTab("New Tab", "about:newtab");
    }
  }

  export function destroyTabs() {
    if (typeof document !== "undefined") {
      document.removeEventListener("keydown", handleKeydown);
    }
  }

  if (typeof window !== "undefined") {
    window.koreTabs = {
      createTab: createTab,
      closeTab: closeTab,
      switchTab: switchTab,
      switchToNextTab: switchToNextTab,
      switchToPrevTab: switchToPrevTab,
      pinTab: pinTab,
      unpinTab: unpinTab,
      getTab: getTab,
      getAllTabs: getAllTabs,
      getPinnedTabs: getPinnedTabs,
      getUnpinnedTabs: getUnpinnedTabs,
      getActiveTab: getActiveTab,
      getActiveTabId: getActiveTabId,
      getTabCount: getTabCount,
      updateTab: updateTab,
      reset: reset,
      initTabs: initTabs,
      destroyTabs: destroyTabs,
    };
  }
})();
