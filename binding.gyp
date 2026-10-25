{
  "targets": [
    {
      "target_name": "ak_pointer",
      "sources": ["native/ak_pointer.cc"],
      "include_dirs": ["<(module_root_dir)/node_modules/node-addon-api"],
      "defines": ["NAPI_CPP_EXCEPTIONS"],
      "cflags_cc": ["-std=c++17", "-fexceptions"],
      "conditions": [
        [
          "OS=='mac'",
          {
            "xcode_settings": {
              "CLANG_CXX_LANGUAGE_STANDARD": "c++17",
              "CLANG_CXX_LIBRARY": "libc++",
              "GCC_ENABLE_CPP_EXCEPTIONS": "YES",
              "MACOSX_DEPLOYMENT_TARGET": "12.0",
              "OTHER_LDFLAGS": [
                "-framework CoreGraphics",
                "-framework CoreFoundation",
                "-framework ApplicationServices"
              ]
            }
          }
        ]
      ]
    }
  ]
}
