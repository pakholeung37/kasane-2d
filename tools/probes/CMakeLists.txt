cmake_minimum_required(VERSION 3.21)
project(kasane_probes LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(KASANE_CUBISM_ROOT "" CACHE PATH "Local Cubism Native SDK root")

# Build PurismCore submodule reference
add_subdirectory(${CMAKE_CURRENT_SOURCE_DIR}/../../modules/purism-core ${CMAKE_CURRENT_BINARY_DIR}/purism)

# 1. Purism Probe
add_executable(kasane_document_purism_probe project_runtime_probe.cpp)
target_link_libraries(kasane_document_purism_probe PRIVATE PurismCore)

# 2. Official Core Probe (if Cubism SDK root provided)
if(KASANE_CUBISM_ROOT)
  if(APPLE)
    set(_cubism_platform macos)
  elseif(UNIX)
    set(_cubism_platform linux)
  elseif(WIN32)
    set(_cubism_platform windows)
  else()
    message(FATAL_ERROR "Unsupported platform for official Core")
  endif()

  set(_cubism_library "${KASANE_CUBISM_ROOT}/Core/lib/${_cubism_platform}/${CMAKE_SYSTEM_PROCESSOR}/libLive2DCubismCore.a")
  if(NOT EXISTS "${_cubism_library}")
    message(FATAL_ERROR "Missing official Core: ${_cubism_library}")
  endif()

  add_executable(kasane_document_official_probe project_runtime_probe.cpp)
  target_compile_definitions(kasane_document_official_probe PRIVATE KASANE_OFFICIAL_CORE)
  target_include_directories(kasane_document_official_probe PRIVATE "${KASANE_CUBISM_ROOT}/Core/include")
  target_link_libraries(kasane_document_official_probe PRIVATE "${_cubism_library}")
endif()
