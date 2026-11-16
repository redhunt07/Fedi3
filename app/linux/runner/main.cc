/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

#include <cstring>

#include <glib.h>

#include "my_application.h"

namespace {

bool file_exists(const char* path) {
  return path != nullptr && g_file_test(path, G_FILE_TEST_EXISTS);
}

bool any_exists(const char* const* paths, size_t count) {
  for (size_t i = 0; i < count; ++i) {
    if (file_exists(paths[i])) {
      return true;
    }
  }
  return false;
}

// Flutter 3.50.0 renders Linux content shifted by half a window on the NVIDIA
// proprietary EGL driver because glBlitFramebuffer is blocklisted there. The
// engine fix ships in Flutter 3.50.1. Until then, when both the NVIDIA and the
// Mesa EGL vendors are installed, prefer Mesa so the window is drawn correctly.
// Packaging launchers do the same; applying it here as well covers the deb and
// any launch path that skips the launcher script.
//
// An explicit __EGL_VENDOR_LIBRARY_FILENAMES is always respected, so a user who
// wants the NVIDIA driver can still opt in.
void prefer_mesa_egl_on_nvidia() {
  if (g_getenv("__EGL_VENDOR_LIBRARY_FILENAMES") != nullptr) {
    return;
  }
  const char* nvidia_vendors[] = {
      "/usr/share/glvnd/egl_vendor.d/10_nvidia.json",
      "/usr/lib/x86_64-linux-gnu/GL/glvnd/egl_vendor.d/10_nvidia.json",
      "/usr/lib/aarch64-linux-gnu/GL/glvnd/egl_vendor.d/10_nvidia.json",
  };
  if (!any_exists(nvidia_vendors, G_N_ELEMENTS(nvidia_vendors))) {
    return;
  }
  // Prefer the host glvnd directory on a normal system, then the Flatpak
  // runtime's own Mesa under GL/default, then the GL extension copy under
  // GL/glvnd. On a Wayland session the sandboxed zink device is unavailable and
  // Mesa falls back to llvmpipe; the Flatpak launcher keeps X11 (where zink
  // works) so rendering stays on the GPU.
  const char* mesa_vendors[] = {
      "/usr/share/glvnd/egl_vendor.d/50_mesa.json",
      "/usr/lib/x86_64-linux-gnu/GL/default/share/glvnd/egl_vendor.d/50_mesa.json",
      "/usr/lib/aarch64-linux-gnu/GL/default/share/glvnd/egl_vendor.d/50_mesa.json",
      "/usr/lib/x86_64-linux-gnu/GL/glvnd/egl_vendor.d/50_mesa.json",
  };
  const char* mesa_vendor = nullptr;
  for (const char* vendor : mesa_vendors) {
    if (file_exists(vendor)) {
      mesa_vendor = vendor;
      break;
    }
  }
  if (mesa_vendor == nullptr) {
    return;
  }
  g_setenv("__EGL_VENDOR_LIBRARY_FILENAMES", mesa_vendor, TRUE);
  if (g_getenv("MESA_LOADER_DRIVER_OVERRIDE") != nullptr) {
    return;
  }
  // Mesa has no native GL driver for the NVIDIA proprietary stack. Prefer zink
  // (GL on Vulkan) so rendering stays on the GPU; fall back to llvmpipe when
  // Vulkan is unavailable. zink must not be selected while
  // LIBGL_ALWAYS_SOFTWARE=1: it then requests a CPU device and fails to create
  // the EGL screen.
  const char* software = g_getenv("LIBGL_ALWAYS_SOFTWARE");
  const bool force_software =
      software != nullptr && std::strcmp(software, "1") == 0;
  const char* vulkan_icds[] = {
      "/usr/share/vulkan/icd.d/nvidia_icd.json",
      "/usr/lib/x86_64-linux-gnu/GL/vulkan/icd.d/nvidia_icd.json",
      "/usr/lib/aarch64-linux-gnu/GL/vulkan/icd.d/nvidia_icd.json",
  };
  const bool has_vulkan =
      any_exists(vulkan_icds, G_N_ELEMENTS(vulkan_icds));
  g_setenv("MESA_LOADER_DRIVER_OVERRIDE",
           (!force_software && has_vulkan) ? "zink" : "llvmpipe", TRUE);
}

}  // namespace

int main(int argc, char** argv) {
  prefer_mesa_egl_on_nvidia();
  g_autoptr(MyApplication) app = my_application_new();
  return g_application_run(G_APPLICATION(app), argc, argv);
}
