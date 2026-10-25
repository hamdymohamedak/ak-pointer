// macOS delivers CGEventTap callbacks on a run loop. The callback has to stay in
// this addon. Crossing into JavaScript on every mouse event gets the tap disabled.

#include <napi.h>

#include <ApplicationServices/ApplicationServices.h>
#include <CoreFoundation/CoreFoundation.h>
#include <CoreGraphics/CoreGraphics.h>

#include <mach-o/dyld.h>
#include <unistd.h>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <limits>
#include <mutex>
#include <optional>
#include <string>
#include <thread>
#include <vector>

namespace {

constexpr int64_t kReplayStamp = 0x00706F696E746572LL;
constexpr uint16_t kKeyEscape = 0x35;
constexpr uint16_t kKeyF8 = 0x64;

using clock = std::chrono::steady_clock;

enum class Kind { Move, Down, Up, Drag, Scroll, Key };
enum class MouseButton { Left, Right, Middle };
enum class ScrollUnit { Line, Pixel };

struct InputEvent {
  int64_t t_ms = 0;
  Kind kind = Kind::Move;
  MouseButton button = MouseButton::Left;
  int click_count = 1;
  double x = 0;
  double y = 0;
  int scroll_x = 0;
  int scroll_y = 0;
  ScrollUnit scroll_unit = ScrollUnit::Line;
  bool key_down = true;
  uint16_t keycode = 0;
  uint64_t flags = 0;
  uint16_t target = 0;
};

struct TargetWindow {
  std::string owner;
  std::string title;
  double w = 0;
  double h = 0;
};

struct Action {
  int version = 1;
  std::string window_owner;
  std::string window_title;
  double window_w = 0;
  double window_h = 0;
  std::vector<TargetWindow> targets;
  std::vector<InputEvent> events;

  TargetWindow target_window(size_t index) const {
    if (index < targets.size()) return targets[index];
    return {window_owner, window_title, window_w, window_h};
  }
};

struct WindowInfo {
  uint32_t id = 0;
  int pid = 0;
  int layer = 0;
  std::string owner;
  std::string title;
  double x = 0;
  double y = 0;
  double w = 0;
  double h = 0;
};

struct AllowedWindow {
  std::string owner;
  std::string title;
};

struct AppChoice {
  std::string owner;
  std::string title;
  double w = 0;
  double h = 0;
  bool visible = false;
};

struct ScreenPoint {
  double x = 0;
  double y = 0;
};

void cf_release(CFTypeRef value) {
  if (value) CFRelease(value);
}

std::string cf_string(CFStringRef value) {
  if (!value) return {};
  const char* fast = CFStringGetCStringPtr(value, kCFStringEncodingUTF8);
  if (fast) return fast;
  CFIndex length = CFStringGetLength(value);
  CFIndex bytes = CFStringGetMaximumSizeForEncoding(length, kCFStringEncodingUTF8) + 1;
  if (bytes < 1) bytes = 1;
  std::string buffer(static_cast<size_t>(bytes), '\0');
  if (!CFStringGetCString(value, buffer.data(), bytes, kCFStringEncodingUTF8)) return {};
  buffer.resize(std::strlen(buffer.c_str()));
  return buffer;
}

CFStringRef cf_str(const char* text) {
  return CFStringCreateWithCString(kCFAllocatorDefault, text, kCFStringEncodingUTF8);
}

bool ignored_owner(const std::string& owner) {
  return owner == "Dock" || owner == "Window Server" || owner == "SystemUIServer" ||
         owner == "Control Center" || owner == "Notification Center" || owner == "Spotlight";
}

int64_t dict_i64(CFDictionaryRef dict, CFStringRef key) {
  auto value = CFDictionaryGetValue(dict, key);
  if (!value || CFGetTypeID(value) != CFNumberGetTypeID()) return 0;
  int64_t out = 0;
  CFNumberGetValue(static_cast<CFNumberRef>(value), kCFNumberSInt64Type, &out);
  return out;
}

double dict_f64(CFDictionaryRef dict, CFStringRef key, double fallback) {
  auto value = CFDictionaryGetValue(dict, key);
  if (!value || CFGetTypeID(value) != CFNumberGetTypeID()) return fallback;
  double out = fallback;
  CFNumberGetValue(static_cast<CFNumberRef>(value), kCFNumberDoubleType, &out);
  return out;
}

std::string dict_string(CFDictionaryRef dict, CFStringRef key) {
  auto value = CFDictionaryGetValue(dict, key);
  if (!value || CFGetTypeID(value) != CFStringGetTypeID()) return {};
  return cf_string(static_cast<CFStringRef>(value));
}

std::optional<WindowInfo> parse_window(CFDictionaryRef dict) {
  if (!dict) return std::nullopt;
  auto bounds = CFDictionaryGetValue(dict, kCGWindowBounds);
  if (!bounds || CFGetTypeID(bounds) != CFDictionaryGetTypeID()) return std::nullopt;
  CGRect rect = CGRectZero;
  if (!CGRectMakeWithDictionaryRepresentation(static_cast<CFDictionaryRef>(bounds), &rect)) {
    return std::nullopt;
  }
  if (dict_f64(dict, kCGWindowAlpha, 1.0) <= 0.01) return std::nullopt;
  WindowInfo info;
  info.id = static_cast<uint32_t>(dict_i64(dict, kCGWindowNumber));
  info.pid = static_cast<int>(dict_i64(dict, kCGWindowOwnerPID));
  info.layer = static_cast<int>(dict_i64(dict, kCGWindowLayer));
  info.owner = dict_string(dict, kCGWindowOwnerName);
  info.title = dict_string(dict, kCGWindowName);
  info.x = rect.origin.x;
  info.y = rect.origin.y;
  info.w = rect.size.width;
  info.h = rect.size.height;
  return info;
}

std::vector<WindowInfo> list_with(CGWindowListOption options) {
  CFArrayRef list = CGWindowListCopyWindowInfo(options, kCGNullWindowID);
  if (!list) return {};
  std::vector<WindowInfo> windows;
  CFIndex count = CFArrayGetCount(list);
  windows.reserve(static_cast<size_t>(std::max<CFIndex>(count, 0)));
  for (CFIndex index = 0; index < count; index++) {
    auto dict = static_cast<CFDictionaryRef>(CFArrayGetValueAtIndex(list, index));
    if (auto info = parse_window(dict)) windows.push_back(*info);
  }
  CFRelease(list);
  return windows;
}

std::vector<WindowInfo> list_windows() {
  return list_with(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements);
}

std::vector<WindowInfo> list_windows_all() {
  return list_with(kCGWindowListExcludeDesktopElements);
}

bool usable_app_window(const WindowInfo& window, int self_pid) {
  return window.layer == 0 && window.w >= 40 && window.h >= 40 && window.pid != 0 &&
         window.pid != self_pid && !ignored_owner(window.owner) && !window.owner.empty();
}

std::vector<AppChoice> list_app_choices() {
  int self_pid = getpid();
  std::vector<AppChoice> choices;
  auto consider = [&](const WindowInfo& window) {
    if (!usable_app_window(window, self_pid)) return;
    for (auto& choice : choices) {
      if (choice.owner == window.owner && choice.title == window.title) {
        if (choice.w * choice.h < window.w * window.h) {
          choice.w = window.w;
          choice.h = window.h;
        }
        return;
      }
    }
    choices.push_back({window.owner, window.title, window.w, window.h, false});
  };
  for (const auto& window : list_windows_all()) consider(window);
  for (const auto& window : list_windows()) {
    for (auto& choice : choices) {
      if (choice.owner == window.owner && choice.title == window.title) choice.visible = true;
    }
  }
  std::sort(choices.begin(), choices.end(), [](const AppChoice& left, const AppChoice& right) {
    if (left.owner != right.owner) return left.owner < right.owner;
    return left.title < right.title;
  });
  return choices;
}

std::optional<WindowInfo> pick_app_window(const std::vector<WindowInfo>& windows, const std::string& owner,
                                          const std::string& title) {
  std::optional<WindowInfo> best;
  double best_area = -1;
  for (const auto& window : windows) {
    if (window.layer != 0 || window.w < 40 || window.h < 40 || window.owner != owner ||
        ignored_owner(window.owner)) {
      continue;
    }
    if (!title.empty() && window.title != title) continue;
    double area = window.w * window.h;
    if (area > best_area) {
      best_area = area;
      best = window;
    }
  }
  return best;
}

std::optional<WindowInfo> find_app_window(const std::string& owner, const std::string& title) {
  if (auto found = pick_app_window(list_windows(), owner, title)) return found;
  return pick_app_window(list_windows_all(), owner, title);
}

std::optional<WindowInfo> front_window() {
  for (const auto& window : list_windows()) {
    if (window.layer == 0 && window.w >= 40 && window.h >= 40 && window.pid != 0 &&
        !ignored_owner(window.owner)) {
      return window;
    }
  }
  return std::nullopt;
}

std::optional<WindowInfo> window_by_id(uint32_t id) {
  if (id == 0) return std::nullopt;
  int64_t wide = id;
  CFNumberRef number = CFNumberCreate(kCFAllocatorDefault, kCFNumberSInt64Type, &wide);
  if (!number) return std::nullopt;
  const void* values[] = {number};
  CFArrayRef ids = CFArrayCreate(kCFAllocatorDefault, values, 1, &kCFTypeArrayCallBacks);
  CFArrayRef described = ids ? CGWindowListCreateDescriptionFromArray(ids) : nullptr;
  std::optional<WindowInfo> result;
  if (described && CFArrayGetCount(described) > 0) {
    result = parse_window(static_cast<CFDictionaryRef>(CFArrayGetValueAtIndex(described, 0)));
  }
  cf_release(described);
  cf_release(ids);
  cf_release(number);
  if (result && result->w < 1) return std::nullopt;
  return result;
}

std::optional<WindowInfo> nearest_window(const std::vector<WindowInfo>& windows, const std::string& owner,
                                         const std::string& title, double recorded_w, double recorded_h) {
  std::optional<WindowInfo> best;
  double best_score = std::numeric_limits<double>::max();
  for (const auto& window : windows) {
    if (window.layer != 0 || window.owner != owner || window.title != title) continue;
    double score = std::abs(window.w - recorded_w) + std::abs(window.h - recorded_h);
    if (score < best_score) {
      best_score = score;
      best = window;
    }
  }
  return best;
}

std::optional<WindowInfo> find_window(const std::string& owner, const std::string& title, double recorded_w,
                                      double recorded_h) {
  if (auto found = nearest_window(list_windows(), owner, title, recorded_w, recorded_h)) return found;
  return nearest_window(list_windows_all(), owner, title, recorded_w, recorded_h);
}

std::string ax_string(AXUIElementRef element, CFStringRef attribute) {
  CFTypeRef raw = nullptr;
  if (AXUIElementCopyAttributeValue(element, attribute, &raw) != kAXErrorSuccess || !raw) return {};
  std::string out = CFGetTypeID(raw) == CFStringGetTypeID() ? cf_string(static_cast<CFStringRef>(raw)) : "";
  CFRelease(raw);
  return out;
}

CGPoint ax_point(AXUIElementRef element, CFStringRef attribute) {
  CGPoint point = CGPointZero;
  CFTypeRef raw = nullptr;
  if (AXUIElementCopyAttributeValue(element, attribute, &raw) != kAXErrorSuccess || !raw) return point;
  if (CFGetTypeID(raw) == AXValueGetTypeID()) {
    AXValueGetValue(static_cast<AXValueRef>(raw), static_cast<AXValueType>(kAXValueCGPointType), &point);
  }
  CFRelease(raw);
  return point;
}

bool raise_window(const WindowInfo& window) {
  AXUIElementRef app = AXUIElementCreateApplication(window.pid);
  if (!app) return false;
  CFStringRef frontmost = cf_str("AXFrontmost");
  CFStringRef hidden = cf_str("AXHidden");
  CFStringRef minimized = cf_str("AXMinimized");
  CFStringRef windows_attr = cf_str("AXWindows");
  CFStringRef title_attr = cf_str("AXTitle");
  CFStringRef position_attr = cf_str("AXPosition");
  CFStringRef raise_action = cf_str("AXRaise");
  CFStringRef main_attr = cf_str("AXMain");
  AXUIElementSetMessagingTimeout(app, 1.5f);
  AXUIElementSetAttributeValue(app, hidden, kCFBooleanFalse);
  AXError front_err = AXUIElementSetAttributeValue(app, frontmost, kCFBooleanTrue);

  CFTypeRef raw = nullptr;
  AXUIElementRef best = nullptr;
  double best_dist = std::numeric_limits<double>::max();
  bool title_matched = false;
  if (AXUIElementCopyAttributeValue(app, windows_attr, &raw) == kAXErrorSuccess && raw &&
      CFGetTypeID(raw) == CFArrayGetTypeID()) {
    CFArrayRef windows = static_cast<CFArrayRef>(raw);
    CFIndex count = CFArrayGetCount(windows);
    for (CFIndex index = 0; index < count; index++) {
      auto candidate = static_cast<AXUIElementRef>(const_cast<void*>(CFArrayGetValueAtIndex(windows, index)));
      std::string title = ax_string(candidate, title_attr);
      bool same_title = title == window.title;
      if (!window.title.empty() && !same_title) continue;
      if (window.title.empty() && !title.empty()) continue;
      CGPoint pos = ax_point(candidate, position_attr);
      double dist = std::pow(pos.x - window.x, 2) + std::pow(pos.y - window.y, 2);
      if (dist < best_dist) {
        best = candidate;
        best_dist = dist;
        title_matched = true;
      }
    }
    if (!title_matched) {
      best = nullptr;
      best_dist = std::numeric_limits<double>::max();
      for (CFIndex index = 0; index < count; index++) {
        auto candidate = static_cast<AXUIElementRef>(const_cast<void*>(CFArrayGetValueAtIndex(windows, index)));
        CGPoint pos = ax_point(candidate, position_attr);
        double dist = std::pow(pos.x - window.x, 2) + std::pow(pos.y - window.y, 2);
        if (dist < best_dist) {
          best = candidate;
          best_dist = dist;
        }
      }
      if (best_dist > 80.0 * 80.0) best = nullptr;
    }
    if (best) {
      AXUIElementSetAttributeValue(best, minimized, kCFBooleanFalse);
      AXUIElementPerformAction(best, raise_action);
      AXUIElementSetAttributeValue(best, main_attr, kCFBooleanTrue);
    }
  }
  cf_release(raw);
  for (CFStringRef value : {frontmost, hidden, minimized, windows_attr, title_attr, position_attr, raise_action,
                            main_attr}) {
    cf_release(value);
  }
  CFRelease(app);
  return front_err == kAXErrorSuccess;
}

bool title_matches(const std::string& wanted, const std::string& actual) {
  if (wanted.empty() || wanted == actual) return true;
  if (actual.empty()) return false;
  return actual.rfind(wanted, 0) == 0 || wanted.rfind(actual, 0) == 0;
}

bool contains(const WindowInfo& window, double x, double y) {
  return window.w >= 1 && window.h >= 1 && x >= window.x && y >= window.y && x < window.x + window.w &&
         y < window.y + window.h;
}

ScreenPoint to_relative(const WindowInfo& locked, const WindowInfo& current, double screen_x, double screen_y) {
  if (current.w < 1 || current.h < 1 || locked.w < 1 || locked.h < 1) {
    return {screen_x - current.x, screen_y - current.y};
  }
  return {(screen_x - current.x) * (locked.w / current.w), (screen_y - current.y) * (locked.h / current.h)};
}

ScreenPoint to_screen(double recorded_w, double recorded_h, const WindowInfo& current, double rel_x, double rel_y) {
  double scale_x = recorded_w > 1 ? current.w / recorded_w : 1;
  double scale_y = recorded_h > 1 ? current.h / recorded_h : 1;
  return {current.x + rel_x * scale_x, current.y + rel_y * scale_y};
}

bool size_changed(double recorded_w, double recorded_h, const WindowInfo& current) {
  return std::abs(current.w - recorded_w) > 2 || std::abs(current.h - recorded_h) > 2;
}

void print_accessibility_help() {
  std::printf("Accessibility permission is required to watch the mouse and keyboard.\n");
  char exe[4096];
  uint32_t size = sizeof(exe);
  if (_NSGetExecutablePath(exe, &size) == 0) std::printf("Enable this program:\n  %s\n", exe);
  std::printf("System Settings → Privacy & Security → Accessibility\n");
  std::printf("Also enable it under Input Monitoring if that list appears.\n");
  std::printf("Then quit and run the program again.\n");
  std::fflush(stdout);
}

std::atomic<bool> g_input_warning{false};

void warn_input_monitoring() {
  bool expected = false;
  if (!g_input_warning.compare_exchange_strong(expected, true)) return;
  CGRequestListenEventAccess();
  std::printf("Input Monitoring is off, so typed keys may not be recorded.\n");
  std::printf("System Settings → Privacy & Security → Input Monitoring\n");
  std::printf("Enable Electron. If you started ak-pointer from Cursor, enable Cursor as well.\n");
  std::printf("Then run this program again. Mouse clicks are still recorded.\n");
  std::fflush(stdout);
}

extern "C" CGEventRef permission_probe(CGEventTapProxy, CGEventType, CGEventRef event, void*) { return event; }

bool listen_tap_works() {
  CGEventMask mask = CGEventMaskBit(kCGEventLeftMouseDown) | CGEventMaskBit(kCGEventKeyDown);
  CFMachPortRef tap = CGEventTapCreate(kCGSessionEventTap, kCGHeadInsertEventTap, kCGEventTapOptionListenOnly, mask,
                                       permission_probe, nullptr);
  if (!tap) return false;
  CFRelease(tap);
  return true;
}

bool ensure_input_permissions() {
  bool trusted = AXIsProcessTrusted() != 0;
  bool listen = CGPreflightListenEventAccess() != 0;
  if (trusted || listen_tap_works()) {
    if (!listen) warn_input_monitoring();
    return true;
  }
  if (!trusted) {
    const void* keys[] = {kAXTrustedCheckOptionPrompt};
    const void* values[] = {kCFBooleanTrue};
    CFDictionaryRef options = CFDictionaryCreate(kCFAllocatorDefault, keys, values, 1, &kCFTypeDictionaryKeyCallBacks,
                                                 &kCFTypeDictionaryValueCallBacks);
    if (options) {
      AXIsProcessTrustedWithOptions(options);
      CFRelease(options);
    }
  }
  if (!listen) CGRequestListenEventAccess();
  print_accessibility_help();
  return false;
}

std::string missing_window_message(const std::string& owner, const std::string& title) {
  std::string message = "Could not find the window";
  if (!title.empty()) message += " \"" + title + "\"";
  message += " owned by " + owner + ". Open that app and try again. It can be in the background.";
  return message;
}

struct Shared {
  std::mutex mu;
  bool recording = false;
  bool record_done = false;
  std::string record_status;
  Action record_action;
  std::thread record_thread;
  bool playing = false;
  bool play_done = false;
  std::string play_status;
  std::string play_message;
  std::thread play_thread;
};

Shared g;
std::atomic<bool> g_stop_record{false};
std::atomic<uintptr_t> g_record_loop{0};
std::atomic<bool> g_stop_play{false};

struct Recorder;

extern "C" CGEventRef record_callback(CGEventTapProxy, CGEventType type, CGEventRef event, void* user);
extern "C" void stop_timer(CFRunLoopTimerRef, void* info);

struct Recorder {
  CFMachPortRef tap = nullptr;
  int self_pid = 0;
  std::vector<AllowedWindow> allowed;
  uint32_t buttons = 0;
  size_t target_index = 0;
  std::vector<WindowInfo> originals;
  bool locked = false;
  WindowInfo original;
  WindowInfo last_seen;
  Action action;
  clock::time_point origin = clock::now();
  bool have_motion = false;
  clock::time_point last_motion = clock::now();
  double last_motion_x = 0;
  double last_motion_y = 0;
  clock::time_point bounds_at = clock::now();

  std::string run(Action& out) {
    self_pid = getpid();
    if (!prelock()) return "no-window";
    CGEventMask mask = CGEventMaskBit(kCGEventMouseMoved) | CGEventMaskBit(kCGEventLeftMouseDown) |
                       CGEventMaskBit(kCGEventLeftMouseUp) | CGEventMaskBit(kCGEventLeftMouseDragged) |
                       CGEventMaskBit(kCGEventRightMouseDown) | CGEventMaskBit(kCGEventRightMouseUp) |
                       CGEventMaskBit(kCGEventRightMouseDragged) | CGEventMaskBit(kCGEventOtherMouseDown) |
                       CGEventMaskBit(kCGEventOtherMouseUp) | CGEventMaskBit(kCGEventOtherMouseDragged) |
                       CGEventMaskBit(kCGEventScrollWheel) | CGEventMaskBit(kCGEventKeyDown) |
                       CGEventMaskBit(kCGEventKeyUp);
    tap = CGEventTapCreate(kCGSessionEventTap, kCGHeadInsertEventTap, kCGEventTapOptionListenOnly, mask,
                           record_callback, this);
    if (!tap) {
      print_accessibility_help();
      return "no-accessibility";
    }
    CFRunLoopSourceRef source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0);
    if (!source) {
      CFRelease(tap);
      tap = nullptr;
      std::printf("Could not start monitoring.\n");
      return "no-accessibility";
    }
    CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
    CGEventTapEnable(tap, true);
    CFRunLoopTimerContext context;
    std::memset(&context, 0, sizeof(context));
    context.info = this;
    CFRunLoopTimerRef timer =
        CFRunLoopTimerCreate(kCFAllocatorDefault, CFAbsoluteTimeGetCurrent() + 0.2, 0.2, 0, 0, stop_timer, &context);
    if (timer) CFRunLoopAddTimer(CFRunLoopGetCurrent(), timer, kCFRunLoopCommonModes);
    origin = clock::now();
    g_record_loop.store(reinterpret_cast<uintptr_t>(CFRunLoopGetCurrent()));
    if (g_stop_record.load()) CFRunLoopStop(CFRunLoopGetCurrent());
    CFRunLoopRun();
    g_record_loop.store(0);
    if (timer) {
      CFRunLoopRemoveTimer(CFRunLoopGetCurrent(), timer, kCFRunLoopCommonModes);
      CFRelease(timer);
    }
    CGEventTapEnable(tap, false);
    CFRunLoopRemoveSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
    CFRelease(source);
    CFRelease(tap);
    tap = nullptr;
    std::printf("Recording stopped with %zu events.\n", action.events.size());
    std::fflush(stdout);
    std::string status = action.events.empty() ? "empty" : "ok";
    out = std::move(action);
    return status;
  }

  void handle(CGEventType type, CGEventRef event) {
    if (g_stop_record.load()) {
      CFRunLoopStop(CFRunLoopGetCurrent());
      return;
    }
    if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
      if (tap) CGEventTapEnable(tap, true);
      return;
    }
    if (!event) return;
    if (type == kCGEventKeyDown || type == kCGEventKeyUp) {
      auto code = static_cast<uint16_t>(CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode));
      if (code == kKeyF8) {
        if (type == kCGEventKeyDown) {
          g_stop_record.store(true);
          CFRunLoopStop(CFRunLoopGetCurrent());
        }
        return;
      }
      auto front = front_window();
      if (!front) return;
      auto target = same_app_target(*front);
      if (!target) return;
      adopt(*target);
      record_event(type, event);
      return;
    }
    CGPoint point = CGEventGetLocation(event);
    refresh_locked_bounds();
    bool inside = locked && contains(last_seen, point.x, point.y);
    if (is_mouse_down(type)) {
      if (!inside) {
        auto window = target_at(point.x, point.y);
        if (!window) return;
        adopt(*window);
      }
      buttons += 1;
    } else if (is_mouse_up(type)) {
      if (buttons == 0 || !locked) return;
      buttons -= 1;
    } else if (is_drag(type)) {
      if (buttons == 0 || !locked) return;
    } else if (!inside) {
      auto window = target_at(point.x, point.y);
      if (!window) return;
      adopt(*window);
    }
    if (locked) record_event(type, event);
  }

  bool prelock() {
    std::optional<WindowInfo> primary;
    for (auto it = allowed.rbegin(); it != allowed.rend(); ++it) {
      if (auto window = find_app_window(it->owner, it->title)) {
        raise_window(*window);
        primary = window;
      }
    }
    if (!primary) return false;
    adopt(*primary);
    return true;
  }

  void refresh_locked_bounds() {
    if (!locked || clock::now() - bounds_at < std::chrono::milliseconds(100)) return;
    bounds_at = clock::now();
    if (auto live = window_by_id(original.id)) last_seen = *live;
  }

  bool is_allowed(const WindowInfo& window) const {
    if (window.id != 0) {
      for (const auto& known : originals) {
        if (known.id == window.id) return true;
      }
    }
    for (const auto& item : allowed) {
      if (item.owner == window.owner && title_matches(item.title, window.title)) return true;
    }
    return false;
  }

  std::optional<WindowInfo> same_app_target(const WindowInfo& front) const {
    if (is_allowed(front)) return front;
    for (const auto& window : list_windows()) {
      if (is_allowed(window) && window.pid != 0 && window.pid == front.pid) return window;
    }
    return std::nullopt;
  }

  std::optional<WindowInfo> target_at(double x, double y) const {
    auto windows = list_windows();
    std::optional<WindowInfo> front;
    for (const auto& window : windows) {
      if (window.layer == 0 && window.pid != self_pid && contains(window, x, y)) {
        front = window;
        break;
      }
    }
    if (!front) return std::nullopt;
    if (is_allowed(*front)) return front;
    if (!same_app_target(*front)) return std::nullopt;
    for (const auto& window : windows) {
      if (is_allowed(window) && contains(window, x, y)) return window;
    }
    return same_app_target(*front);
  }

  void adopt(WindowInfo live) {
    for (size_t index = 0; index < originals.size(); index++) {
      const auto& known = originals[index];
      if (known.id == live.id || (known.owner == live.owner && known.title == live.title)) {
        target_index = index;
        original = known;
        last_seen = live;
        locked = true;
        bounds_at = clock::now();
        return;
      }
    }
    originals.push_back(live);
    target_index = originals.size() - 1;
    action.targets.push_back({live.owner, live.title, live.w, live.h});
    if (target_index == 0) {
      action.window_owner = live.owner;
      action.window_title = live.title;
      action.window_w = live.w;
      action.window_h = live.h;
    }
    original = live;
    last_seen = live;
    locked = true;
    bounds_at = clock::now();
    std::printf("Tracking window: %s", live.owner.c_str());
    if (!live.title.empty()) std::printf(" - %s", live.title.c_str());
    std::printf("\n");
    std::fflush(stdout);
  }

  void record_event(CGEventType type, CGEventRef event) {
    CGPoint point = CGEventGetLocation(event);
    bool motion = type == kCGEventMouseMoved || type == kCGEventLeftMouseDragged ||
                  type == kCGEventRightMouseDragged || type == kCGEventOtherMouseDragged;
    if (motion && !keep_motion(point.x, point.y)) return;
    ScreenPoint relative = to_relative(original, last_seen, point.x, point.y);
    InputEvent stored;
    stored.t_ms = std::chrono::duration_cast<std::chrono::milliseconds>(clock::now() - origin).count();
    stored.x = relative.x;
    stored.y = relative.y;
    stored.target = static_cast<uint16_t>(target_index);
    if (motion) {
      stored.kind = type == kCGEventMouseMoved ? Kind::Move : Kind::Drag;
      stored.button = button_for(type);
    } else if (is_mouse_down(type) || is_mouse_up(type)) {
      stored.kind = is_mouse_down(type) ? Kind::Down : Kind::Up;
      stored.button = button_for(type);
      int64_t clicks = CGEventGetIntegerValueField(event, kCGMouseEventClickState);
      stored.click_count = clicks > 0 ? static_cast<int>(clicks) : 1;
      note_motion(point.x, point.y);
    } else if (type == kCGEventScrollWheel) {
      int point_x = static_cast<int>(CGEventGetIntegerValueField(event, kCGScrollWheelEventPointDeltaAxis2));
      int point_y = static_cast<int>(CGEventGetIntegerValueField(event, kCGScrollWheelEventPointDeltaAxis1));
      int line_x = static_cast<int>(CGEventGetIntegerValueField(event, kCGScrollWheelEventDeltaAxis2));
      int line_y = static_cast<int>(CGEventGetIntegerValueField(event, kCGScrollWheelEventDeltaAxis1));
      stored.kind = Kind::Scroll;
      if (point_x != 0 || point_y != 0) {
        stored.scroll_x = point_x;
        stored.scroll_y = point_y;
        stored.scroll_unit = ScrollUnit::Pixel;
      } else if (line_x != 0 || line_y != 0) {
        stored.scroll_x = line_x;
        stored.scroll_y = line_y;
        stored.scroll_unit = ScrollUnit::Line;
      } else {
        return;
      }
    } else if (type == kCGEventKeyDown || type == kCGEventKeyUp) {
      stored.kind = Kind::Key;
      stored.key_down = type == kCGEventKeyDown;
      stored.keycode = static_cast<uint16_t>(CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode));
      stored.flags = CGEventGetFlags(event);
    } else {
      return;
    }
    action.events.push_back(stored);
  }

  bool keep_motion(double x, double y) {
    if (!have_motion) {
      note_motion(x, y);
      return true;
    }
    double dx = x - last_motion_x;
    double dy = y - last_motion_y;
    auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(clock::now() - last_motion).count();
    if (ms < 4 && dx * dx + dy * dy < 1.0) return false;
    note_motion(x, y);
    return true;
  }

  void note_motion(double x, double y) {
    have_motion = true;
    last_motion = clock::now();
    last_motion_x = x;
    last_motion_y = y;
  }

  static bool is_mouse_down(CGEventType type) {
    return type == kCGEventLeftMouseDown || type == kCGEventRightMouseDown || type == kCGEventOtherMouseDown;
  }
  static bool is_mouse_up(CGEventType type) {
    return type == kCGEventLeftMouseUp || type == kCGEventRightMouseUp || type == kCGEventOtherMouseUp;
  }
  static bool is_drag(CGEventType type) {
    return type == kCGEventLeftMouseDragged || type == kCGEventRightMouseDragged ||
           type == kCGEventOtherMouseDragged;
  }
  static MouseButton button_for(CGEventType type) {
    if (type == kCGEventRightMouseDown || type == kCGEventRightMouseUp || type == kCGEventRightMouseDragged) {
      return MouseButton::Right;
    }
    if (type == kCGEventOtherMouseDown || type == kCGEventOtherMouseUp || type == kCGEventOtherMouseDragged) {
      return MouseButton::Middle;
    }
    return MouseButton::Left;
  }
};

extern "C" CGEventRef record_callback(CGEventTapProxy, CGEventType type, CGEventRef event, void* user) {
  if (user) static_cast<Recorder*>(user)->handle(type, event);
  return event;
}

extern "C" void stop_timer(CFRunLoopTimerRef, void* info) {
  if (!g_stop_record.load() || !info) return;
  CFRunLoopStop(CFRunLoopGetCurrent());
}

struct WatchState {
  std::atomic<bool> cancel{false};
  std::atomic<bool> stop{false};
  std::atomic<bool> ready{false};
  std::atomic<bool> active{false};
  std::atomic<uintptr_t> tap{0};
};

extern "C" CGEventRef escape_callback(CGEventTapProxy, CGEventType type, CGEventRef event, void* user) {
  auto* state = static_cast<WatchState*>(user);
  if (!state) return event;
  if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
    auto tap = reinterpret_cast<CFMachPortRef>(state->tap.load());
    if (tap) CGEventTapEnable(tap, true);
    return event;
  }
  if (!event || type != kCGEventKeyDown) return event;
  if (CGEventGetIntegerValueField(event, kCGEventSourceUserData) == kReplayStamp) return event;
  auto code = static_cast<uint16_t>(CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode));
  if (code == kKeyEscape) state->cancel.store(true);
  return event;
}

void watcher_thread(WatchState* state) {
  CFMachPortRef tap =
      CGEventTapCreate(kCGSessionEventTap, kCGHeadInsertEventTap, kCGEventTapOptionListenOnly,
                       CGEventMaskBit(kCGEventKeyDown), escape_callback, state);
  if (!tap) {
    state->ready.store(true);
    return;
  }
  CFRunLoopSourceRef source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0);
  if (!source) {
    CFRelease(tap);
    state->ready.store(true);
    return;
  }
  state->tap.store(reinterpret_cast<uintptr_t>(tap));
  CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
  CGEventTapEnable(tap, true);
  state->active.store(true);
  state->ready.store(true);
  while (!state->stop.load()) CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, true);
  CGEventTapEnable(tap, false);
  CFRunLoopRemoveSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
  CFRelease(source);
  CFRelease(tap);
  state->tap.store(0);
}

struct EscapeWatcher {
  WatchState* state = new WatchState();
  std::thread thread;
  EscapeWatcher() {
    WatchState* raw = state;
    thread = std::thread([raw] { watcher_thread(raw); });
    auto deadline = clock::now() + std::chrono::seconds(2);
    while (!state->ready.load() && clock::now() < deadline) {
      std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
  }
  EscapeWatcher(const EscapeWatcher&) = delete;
  ~EscapeWatcher() {
    state->stop.store(true);
    if (thread.joinable()) thread.join();
    delete state;
  }
  bool cancelled() const { return state->cancel.load() || g_stop_play.load(); }
  bool active() const { return state->active.load(); }
};

bool wait_until(clock::time_point target, const EscapeWatcher& watcher) {
  while (!watcher.cancelled()) {
    if (clock::now() >= target) return true;
    auto remaining = target - clock::now();
    if (remaining > std::chrono::milliseconds(30)) {
      std::this_thread::sleep_for(std::chrono::milliseconds(30));
    } else if (remaining > clock::duration::zero()) {
      std::this_thread::sleep_for(remaining);
    }
  }
  return false;
}

void stamp(CGEventRef event) { CGEventSetIntegerValueField(event, kCGEventSourceUserData, kReplayStamp); }

CGEventType mouse_type(const InputEvent& event) {
  bool right = event.button == MouseButton::Right;
  bool middle = event.button == MouseButton::Middle;
  if (event.kind == Kind::Down) {
    return right ? kCGEventRightMouseDown : middle ? kCGEventOtherMouseDown : kCGEventLeftMouseDown;
  }
  if (event.kind == Kind::Up) {
    return right ? kCGEventRightMouseUp : middle ? kCGEventOtherMouseUp : kCGEventLeftMouseUp;
  }
  if (event.kind == Kind::Drag) {
    return right ? kCGEventRightMouseDragged : middle ? kCGEventOtherMouseDragged : kCGEventLeftMouseDragged;
  }
  return kCGEventMouseMoved;
}

CGMouseButton cg_button(MouseButton button) {
  if (button == MouseButton::Right) return kCGMouseButtonRight;
  if (button == MouseButton::Middle) return kCGMouseButtonCenter;
  return kCGMouseButtonLeft;
}

void post_mouse(CGEventSourceRef source, CGEventType type, CGPoint point, CGMouseButton button, int click_count,
                CGPoint& last) {
  CGEventRef event = CGEventCreateMouseEvent(source, type, point, button);
  if (!event) return;
  stamp(event);
  if (type != kCGEventMouseMoved) {
    CGEventSetIntegerValueField(event, kCGMouseEventClickState, click_count);
  }
  CGEventSetIntegerValueField(event, kCGMouseEventDeltaX, std::llround(point.x - last.x));
  CGEventSetIntegerValueField(event, kCGMouseEventDeltaY, std::llround(point.y - last.y));
  last = point;
  CGEventPost(kCGHIDEventTap, event);
  CFRelease(event);
}

struct PlayOutcome {
  std::string status;
  std::string message;
};

PlayOutcome run_play(const Action& action, int countdown) {
  if (action.events.empty()) return {"done", "The action is empty."};
  auto window = find_window(action.window_owner, action.window_title, action.window_w, action.window_h);
  if (!window) return {"no-window", missing_window_message(action.window_owner, action.window_title)};
  std::printf("Playing on: %s", window->owner.c_str());
  if (!window->title.empty()) std::printf(" / %s", window->title.c_str());
  std::printf("\n");
  if (size_changed(action.window_w, action.window_h, *window)) {
    std::printf("Window size changed from %lldx%lld to %lldx%lld. Coordinates will be scaled.\n",
                std::llround(action.window_w), std::llround(action.window_h), std::llround(window->w),
                std::llround(window->h));
  }
  std::fflush(stdout);
  EscapeWatcher watcher;
  if (!watcher.active()) std::printf("Esc cannot cancel this run. Playback will continue.\n");
  if (!raise_window(*window)) std::printf("Could not raise the window. Keystrokes may not reach it.\n");
  if (countdown > 0) {
    std::printf("Starting in %d seconds. Press Esc or Stop to cancel.\n", countdown);
    std::fflush(stdout);
    for (int second = countdown; second >= 1; second--) {
      std::printf("%d\n", second);
      std::fflush(stdout);
      if (g_stop_play.load() || !wait_until(clock::now() + std::chrono::seconds(1), watcher)) {
        return {"cancelled", "Stopped."};
      }
    }
  }
  if (auto again = window_by_id(window->id)) window = again;
  raise_window(*window);
  CGEventSourceRef source = CGEventSourceCreate(kCGEventSourceStateHIDSystemState);
  if (!source) return {"no-accessibility", "Could not send events."};
  CGEventSourceSetLocalEventsSuppressionInterval(source, 0);
  CGEventSourceSetUserData(source, kReplayStamp);
  CGPoint last = CGPointZero;
  if (CGEventRef cursor = CGEventCreate(nullptr)) {
    last = CGEventGetLocation(cursor);
    CFRelease(cursor);
  }
  auto start = clock::now();
  int64_t first_ms = action.events.front().t_ms;
  for (const auto& event : action.events) first_ms = std::min(first_ms, event.t_ms);
  std::vector<std::optional<WindowInfo>> windows;
  size_t current_index = static_cast<size_t>(-1);
  for (const auto& event : action.events) {
    auto at = std::chrono::milliseconds(std::max<int64_t>(0, event.t_ms - first_ms));
    if (!wait_until(start + at, watcher)) {
      CFRelease(source);
      return {"cancelled", "Stopped."};
    }
    size_t index = event.target;
    if (current_index != static_cast<size_t>(-1) && index != current_index) {
      if (g_stop_play.load() || !wait_until(clock::now() + std::chrono::milliseconds(200), watcher)) {
        CFRelease(source);
        return {"cancelled", "Stopped."};
      }
    }
    if (windows.size() <= index) windows.resize(index + 1);
    if (!windows[index] || index != current_index) {
      TargetWindow spec = action.target_window(index);
      auto found = find_window(spec.owner, spec.title, spec.w, spec.h);
      if (!found) {
        CFRelease(source);
        return {"no-window", missing_window_message(spec.owner, spec.title)};
      }
      raise_window(*found);
      windows[index] = found;
      current_index = index;
    }
    TargetWindow spec = action.target_window(index);
    WindowInfo stored = *windows[index];
    WindowInfo current = window_by_id(stored.id).value_or(stored);
    windows[index] = current;
    ScreenPoint screen = to_screen(spec.w, spec.h, current, event.x, event.y);
    CGPoint point = CGPointMake(screen.x, screen.y);
    if (event.kind == Kind::Key) {
      CGEventRef key = CGEventCreateKeyboardEvent(source, event.keycode, event.key_down);
      if (key) {
        CGEventSetFlags(key, event.flags);
        stamp(key);
        CGEventPost(kCGHIDEventTap, key);
        CFRelease(key);
      }
      continue;
    }
    if (event.kind == Kind::Scroll) {
      if (std::abs(last.x - point.x) > 0.5 || std::abs(last.y - point.y) > 0.5) {
        post_mouse(source, kCGEventMouseMoved, point, kCGMouseButtonLeft, 0, last);
      }
      CGScrollEventUnit unit =
          event.scroll_unit == ScrollUnit::Pixel ? kCGScrollEventUnitPixel : kCGScrollEventUnitLine;
      CGEventRef scroll = CGEventCreateScrollWheelEvent(source, unit, 2, event.scroll_y, event.scroll_x);
      if (scroll) {
        CGEventSetLocation(scroll, point);
        stamp(scroll);
        CGEventPost(kCGHIDEventTap, scroll);
        CFRelease(scroll);
      }
      continue;
    }
    post_mouse(source, mouse_type(event), point, cg_button(event.button), event.click_count, last);
  }
  CFRelease(source);
  if (watcher.cancelled()) return {"cancelled", "Stopped."};
  return {"done", "Played."};
}

std::string js_string(Napi::Value value) {
  if (!value.IsString()) return {};
  return value.As<Napi::String>().Utf8Value();
}

double js_number(Napi::Object object, const char* key, double fallback) {
  if (!object.Has(key) || !object.Get(key).IsNumber()) return fallback;
  return object.Get(key).As<Napi::Number>().DoubleValue();
}

std::string js_field(Napi::Object object, const char* key) {
  if (!object.Has(key)) return {};
  return js_string(object.Get(key));
}

const char* kind_name(Kind kind) {
  switch (kind) {
    case Kind::Down:
      return "down";
    case Kind::Up:
      return "up";
    case Kind::Drag:
      return "drag";
    case Kind::Scroll:
      return "scroll";
    case Kind::Key:
      return "key";
    case Kind::Move:
      return "move";
  }
  return "move";
}

const char* button_name(MouseButton button) {
  if (button == MouseButton::Right) return "right";
  if (button == MouseButton::Middle) return "middle";
  return "left";
}

Napi::Object action_to_js(Napi::Env env, const Action& action) {
  auto object = Napi::Object::New(env);
  object.Set("version", Napi::Number::New(env, action.version));
  object.Set("windowOwner", Napi::String::New(env, action.window_owner));
  object.Set("windowTitle", Napi::String::New(env, action.window_title));
  object.Set("windowW", Napi::Number::New(env, action.window_w));
  object.Set("windowH", Napi::Number::New(env, action.window_h));
  auto targets = Napi::Array::New(env, action.targets.size());
  for (uint32_t index = 0; index < action.targets.size(); index++) {
    auto target = Napi::Object::New(env);
    target.Set("owner", Napi::String::New(env, action.targets[index].owner));
    target.Set("title", Napi::String::New(env, action.targets[index].title));
    target.Set("w", Napi::Number::New(env, action.targets[index].w));
    target.Set("h", Napi::Number::New(env, action.targets[index].h));
    targets.Set(index, target);
  }
  object.Set("targets", targets);
  auto events = Napi::Array::New(env, action.events.size());
  for (uint32_t index = 0; index < action.events.size(); index++) {
    const auto& event = action.events[index];
    auto item = Napi::Object::New(env);
    item.Set("tMs", Napi::Number::New(env, static_cast<double>(event.t_ms)));
    item.Set("kind", Napi::String::New(env, kind_name(event.kind)));
    item.Set("button", Napi::String::New(env, button_name(event.button)));
    item.Set("clickCount", Napi::Number::New(env, event.click_count));
    item.Set("x", Napi::Number::New(env, event.x));
    item.Set("y", Napi::Number::New(env, event.y));
    item.Set("scrollX", Napi::Number::New(env, event.scroll_x));
    item.Set("scrollY", Napi::Number::New(env, event.scroll_y));
    item.Set("scrollUnit", Napi::String::New(env, event.scroll_unit == ScrollUnit::Pixel ? "pixel" : "line"));
    item.Set("keyDown", Napi::Boolean::New(env, event.key_down));
    item.Set("keycode", Napi::Number::New(env, event.keycode));
    item.Set("flags", Napi::Number::New(env, static_cast<double>(event.flags)));
    item.Set("target", Napi::Number::New(env, event.target));
    events.Set(index, item);
  }
  object.Set("events", events);
  return object;
}

Kind kind_from(const std::string& name) {
  if (name == "down") return Kind::Down;
  if (name == "up") return Kind::Up;
  if (name == "drag") return Kind::Drag;
  if (name == "scroll") return Kind::Scroll;
  if (name == "key") return Kind::Key;
  return Kind::Move;
}

MouseButton button_from(const std::string& name) {
  if (name == "right") return MouseButton::Right;
  if (name == "middle") return MouseButton::Middle;
  return MouseButton::Left;
}

Action action_from_js(Napi::Value value) {
  Action action;
  if (!value.IsObject()) return action;
  auto object = value.As<Napi::Object>();
  action.version = static_cast<int>(js_number(object, "version", 1));
  action.window_owner = js_field(object, "windowOwner");
  action.window_title = js_field(object, "windowTitle");
  action.window_w = js_number(object, "windowW", 0);
  action.window_h = js_number(object, "windowH", 0);
  if (object.Has("targets") && object.Get("targets").IsArray()) {
    auto targets = object.Get("targets").As<Napi::Array>();
    for (uint32_t index = 0; index < targets.Length(); index++) {
      if (!targets.Get(index).IsObject()) continue;
      auto item = targets.Get(index).As<Napi::Object>();
      action.targets.push_back(
          {js_field(item, "owner"), js_field(item, "title"), js_number(item, "w", 0), js_number(item, "h", 0)});
    }
  }
  if (object.Has("events") && object.Get("events").IsArray()) {
    auto events = object.Get("events").As<Napi::Array>();
    for (uint32_t index = 0; index < events.Length(); index++) {
      if (!events.Get(index).IsObject()) continue;
      auto item = events.Get(index).As<Napi::Object>();
      InputEvent event;
      event.t_ms = static_cast<int64_t>(js_number(item, "tMs", 0));
      event.kind = kind_from(js_field(item, "kind"));
      event.button = button_from(js_field(item, "button"));
      event.click_count = static_cast<int>(js_number(item, "clickCount", 1));
      event.x = js_number(item, "x", 0);
      event.y = js_number(item, "y", 0);
      event.scroll_x = static_cast<int>(js_number(item, "scrollX", 0));
      event.scroll_y = static_cast<int>(js_number(item, "scrollY", 0));
      event.scroll_unit = js_field(item, "scrollUnit") == "pixel" ? ScrollUnit::Pixel : ScrollUnit::Line;
      event.key_down = !item.Has("keyDown") || !item.Get("keyDown").IsBoolean() || item.Get("keyDown").As<Napi::Boolean>().Value();
      event.keycode = static_cast<uint16_t>(js_number(item, "keycode", 0));
      event.flags = static_cast<uint64_t>(js_number(item, "flags", 0));
      event.target = static_cast<uint16_t>(js_number(item, "target", 0));
      action.events.push_back(event);
    }
  }
  return action;
}

Napi::Object status_object(Napi::Env env, bool ok, const std::string& status) {
  auto object = Napi::Object::New(env);
  object.Set("ok", Napi::Boolean::New(env, ok));
  object.Set("status", Napi::String::New(env, status));
  return object;
}

bool busy_locked() {
  std::lock_guard<std::mutex> lock(g.mu);
  return (g.recording && !g.record_done) || (g.playing && !g.play_done);
}

void join_finished_record() {
  bool join = false;
  {
    std::lock_guard<std::mutex> lock(g.mu);
    join = !g.recording || g.record_done;
  }
  if (join && g.record_thread.joinable()) g.record_thread.join();
}

void join_finished_play() {
  bool join = false;
  {
    std::lock_guard<std::mutex> lock(g.mu);
    join = !g.playing || g.play_done;
  }
  if (join && g.play_thread.joinable()) g.play_thread.join();
}

Napi::Value ListApps(const Napi::CallbackInfo& info) {
  auto env = info.Env();
  auto apps = list_app_choices();
  auto array = Napi::Array::New(env, apps.size());
  for (uint32_t index = 0; index < apps.size(); index++) {
    auto item = Napi::Object::New(env);
    item.Set("owner", Napi::String::New(env, apps[index].owner));
    item.Set("title", Napi::String::New(env, apps[index].title));
    item.Set("w", Napi::Number::New(env, apps[index].w));
    item.Set("h", Napi::Number::New(env, apps[index].h));
    item.Set("visible", Napi::Boolean::New(env, apps[index].visible));
    array.Set(index, item);
  }
  return array;
}

Napi::Value KeyboardReady(const Napi::CallbackInfo& info) {
  return Napi::Boolean::New(info.Env(), CGPreflightListenEventAccess() != 0);
}

Napi::Value StartRecording(const Napi::CallbackInfo& info) {
  auto env = info.Env();
  if (info.Length() < 1 || !info[0].IsArray()) return status_object(env, false, "bad-args");
  if (!ensure_input_permissions()) return status_object(env, false, "no-accessibility");
  join_finished_record();
  join_finished_play();
  if (busy_locked()) return status_object(env, false, "busy");
  std::vector<AllowedWindow> allowed;
  auto apps = info[0].As<Napi::Array>();
  for (uint32_t index = 0; index < apps.Length(); index++) {
    if (!apps.Get(index).IsObject()) continue;
    auto item = apps.Get(index).As<Napi::Object>();
    AllowedWindow window;
    window.owner = js_field(item, "owner");
    window.title = js_field(item, "title");
    if (!window.owner.empty()) allowed.push_back(window);
  }
  if (allowed.empty()) return status_object(env, false, "no-window");
  {
    std::lock_guard<std::mutex> lock(g.mu);
    if ((g.recording && !g.record_done) || (g.playing && !g.play_done)) return status_object(env, false, "busy");
    g.recording = true;
    g.record_done = false;
    g.record_status.clear();
    g.record_action = Action();
  }
  g_stop_record.store(false);
  g.record_thread = std::thread([allowed = std::move(allowed)] {
    Action action;
    std::string status = "no-accessibility";
    try {
      Recorder recorder;
      recorder.allowed = allowed;
      status = recorder.run(action);
      if (!action.events.empty()) {
        int64_t start = action.events.front().t_ms;
        for (const auto& event : action.events) start = std::min(start, event.t_ms);
        for (auto& event : action.events) event.t_ms -= start;
      }
    } catch (...) {
      status = "no-accessibility";
    }
    std::lock_guard<std::mutex> lock(g.mu);
    g.record_action = std::move(action);
    g.record_status = status;
    g.record_done = true;
    g.recording = false;
  });
  return status_object(env, true, "ok");
}

Napi::Value StopRecording(const Napi::CallbackInfo& info) {
  g_stop_record.store(true);
  auto loop = reinterpret_cast<CFRunLoopRef>(g_record_loop.load());
  if (loop) CFRunLoopStop(loop);
  return info.Env().Undefined();
}

Napi::Value PollRecording(const Napi::CallbackInfo& info) {
  auto env = info.Env();
  Action action;
  std::string status;
  {
    std::lock_guard<std::mutex> lock(g.mu);
    if (!g.record_done) return env.Null();
    status = g.record_status;
    action = std::move(g.record_action);
    g.record_done = false;
    g.recording = false;
  }
  if (g.record_thread.joinable()) g.record_thread.join();
  auto object = Napi::Object::New(env);
  object.Set("status", Napi::String::New(env, status));
  if (status == "ok" || status == "empty") object.Set("action", action_to_js(env, action));
  return object;
}

Napi::Value StartPlayback(const Napi::CallbackInfo& info) {
  auto env = info.Env();
  if (info.Length() < 1) return status_object(env, false, "bad-args");
  if (!ensure_input_permissions()) return status_object(env, false, "no-accessibility");
  join_finished_play();
  join_finished_record();
  if (busy_locked()) return status_object(env, false, "busy");
  Action action = action_from_js(info[0]);
  int countdown = info.Length() > 1 && info[1].IsNumber() ? info[1].As<Napi::Number>().Int32Value() : 0;
  {
    std::lock_guard<std::mutex> lock(g.mu);
    if ((g.recording && !g.record_done) || (g.playing && !g.play_done)) return status_object(env, false, "busy");
    g.playing = true;
    g.play_done = false;
    g.play_status.clear();
    g.play_message.clear();
  }
  g_stop_play.store(false);
  g.play_thread = std::thread([action = std::move(action), countdown] {
    PlayOutcome outcome{"no-accessibility", "Could not send events."};
    try {
      outcome = run_play(action, countdown);
    } catch (...) {
      outcome = {"no-accessibility", "Could not send events."};
    }
    std::lock_guard<std::mutex> lock(g.mu);
    g.play_status = outcome.status;
    g.play_message = outcome.message;
    g.play_done = true;
    g.playing = false;
  });
  return status_object(env, true, "ok");
}

Napi::Value StopPlayback(const Napi::CallbackInfo& info) {
  g_stop_play.store(true);
  return info.Env().Undefined();
}

Napi::Value PollPlayback(const Napi::CallbackInfo& info) {
  auto env = info.Env();
  std::string status;
  std::string message;
  {
    std::lock_guard<std::mutex> lock(g.mu);
    if (!g.play_done) return env.Null();
    status = g.play_status;
    message = g.play_message;
    g.play_done = false;
    g.playing = false;
  }
  if (g.play_thread.joinable()) g.play_thread.join();
  auto object = Napi::Object::New(env);
  object.Set("status", Napi::String::New(env, status));
  object.Set("message", Napi::String::New(env, message));
  return object;
}

}  // namespace

Napi::Object Init(Napi::Env env, Napi::Object exports) {
  exports.Set("listApps", Napi::Function::New(env, ListApps));
  exports.Set("keyboardReady", Napi::Function::New(env, KeyboardReady));
  exports.Set("startRecording", Napi::Function::New(env, StartRecording));
  exports.Set("stopRecording", Napi::Function::New(env, StopRecording));
  exports.Set("pollRecording", Napi::Function::New(env, PollRecording));
  exports.Set("startPlayback", Napi::Function::New(env, StartPlayback));
  exports.Set("stopPlayback", Napi::Function::New(env, StopPlayback));
  exports.Set("pollPlayback", Napi::Function::New(env, PollPlayback));
  return exports;
}

NODE_API_MODULE(ak_pointer, Init)
