/* check-only stand-in: lets ring's C compile for x86_64-pc-windows-msvc on a Mac (cargo check, never linked) */
#pragma once
#define assert(x) ((void)0)
#define static_assert _Static_assert
