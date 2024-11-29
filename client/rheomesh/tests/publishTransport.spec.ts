import { adjustExtmap } from "../src/publishTransport";
import fs from "fs";
import path from "path";
import * as sdpTransform from "sdp-transform";

describe("adjustExtmap", () => {
  describe("single video sdp", () => {
    it("extmap should be remapped", () => {
      const originalPath = path.join(__dirname, "sdp/sdp_video_original");
      const originalContent = fs.readFileSync(originalPath, "utf-8");
      const originalSdp = new RTCSessionDescription({
        type: "offer",
        sdp: originalContent,
      });
      const res = adjustExtmap(originalSdp);
      const correctPath = path.join(__dirname, "sdp/sdp_video_correct");
      const correctContent = fs.readFileSync(correctPath, "utf-8");

      // Compare extmap
      const adjusted = sdpTransform.parse(res.sdp as string);
      const correct = sdpTransform.parse(correctContent);
      expect(adjusted).toEqual(correct);
    });
  });
  describe("single audio sdp", () => {
    it("extmap should be remapped", () => {
      const originalPath = path.join(__dirname, "sdp/sdp_audio_original");
      const originalContent = fs.readFileSync(originalPath, "utf-8");
      const originalSdp = new RTCSessionDescription({
        type: "offer",
        sdp: originalContent,
      });
      const res = adjustExtmap(originalSdp);
      const correctPath = path.join(__dirname, "sdp/sdp_audio_correct");
      const correctContent = fs.readFileSync(correctPath, "utf-8");

      // Compare extmap
      const adjusted = sdpTransform.parse(res.sdp as string);
      const correct = sdpTransform.parse(correctContent);
      expect(adjusted).toEqual(correct);
    });
  });
  describe("video and audio sdp", () => {
    it("extmap should be remapped", () => {
      const originalPath = path.join(__dirname, "sdp/sdp_video_audio_original");
      const originalContent = fs.readFileSync(originalPath, "utf-8");
      const originalSdp = new RTCSessionDescription({
        type: "offer",
        sdp: originalContent,
      });
      const res = adjustExtmap(originalSdp);
      const correctPath = path.join(__dirname, "sdp/sdp_video_audio_correct");
      const correctContent = fs.readFileSync(correctPath, "utf-8");

      // Compare extmap
      const adjusted = sdpTransform.parse(res.sdp as string);
      const correct = sdpTransform.parse(correctContent);
      expect(adjusted).toEqual(correct);
    });
  });
  describe("audio and video sdp", () => {
    it("extmap should be remapped", () => {
      const originalPath = path.join(__dirname, "sdp/sdp_audio_video_original");
      const originalContent = fs.readFileSync(originalPath, "utf-8");
      const originalSdp = new RTCSessionDescription({
        type: "offer",
        sdp: originalContent,
      });
      const res = adjustExtmap(originalSdp);
      const correctPath = path.join(__dirname, "sdp/sdp_audio_video_correct");
      const correctContent = fs.readFileSync(correctPath, "utf-8");

      // Compare extmap
      const adjusted = sdpTransform.parse(res.sdp as string);
      const correct = sdpTransform.parse(correctContent);
      expect(adjusted).toEqual(correct);
    });
  });
});
