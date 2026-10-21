import { describe, expect, it, vi } from "vitest";

// fileUtils imports Tauri IPC helpers at module load; they are never invoked
// by isImageFile but must resolve inside jsdom.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: vi.fn((path: string) => `asset://localhost/${path}`),
}));

const { isImageFile } = await import("@/lib/fileUtils");

describe("isImageFile", () => {
  it.each([
    "photo.png",
    "photo.JPG",
    "photo.jpeg",
    "anim.gif",
    "vector.svg",
    "icon.ico",
    "image.bmp",
    "image.webp",
    "/tmp/some dir/image.PnG",
  ])("returns true for image path %s", (path) => {
    expect(isImageFile(path)).toBe(true);
  });

  it.each(["notes.md", "code.ts", "archive.tar.gz", "README", "", "/tmp/file."])(
    "returns false for non-image path %s",
    (path) => {
      expect(isImageFile(path)).toBe(false);
    },
  );
});
