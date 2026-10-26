"""观察器必须拒绝误配输入，并在持续供帧时有界排空预热队列。"""
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location("rtsp_sampling",Path(__file__).with_name("benchmark-rtsp-sampling.py"))
h=importlib.util.module_from_spec(spec)
spec.loader.exec_module(h)


class SamplingObserverTests(unittest.TestCase):
    def args(self,codec="hevc",size=None):
        return SimpleNamespace(input=Path("synthetic.mp4"),codec=codec,expected_size=size or [1920,1080])

    def test_input_probe_matches_actual_codec_and_dimensions(self):
        video=dict(codec_name="hevc",width=1920,height=1080,avg_frame_rate="30/1")
        with patch.object(h.subprocess,"check_output",return_value=json.dumps(dict(streams=[video]))):
            self.assertEqual(h.input_description(self.args()),video)
            with self.assertRaises(ValueError): h.input_description(self.args(codec="h264"))
            with self.assertRaises(ValueError): h.input_description(self.args(size=[1280,720]))

    def test_invalid_size_and_missing_video_cannot_look_like_a_measured_success(self):
        with self.assertRaises(ValueError): h.input_description(self.args(size=[8193,1080]))
        with patch.object(h.subprocess,"check_output",return_value='{"streams": []}'):
            with self.assertRaises(ValueError): h.input_description(self.args())

    def test_warmup_drain_is_bounded_even_with_a_continuous_producer(self):
        accepted=[]
        def release(pointer):
            accepted.append(pointer)
            return h.a.AM_OK
        lib=SimpleNamespace(amc_encoded_pull=lambda *args:h.a.AM_OK,
            amc_frame_pull=lambda *args:h.a.AM_OK,
            amc_encoded_packet_release=release,amc_frame_release=release)
        result=h.drain_warmup(lib,object(),[object(),object()])
        self.assertEqual((result["encoded"],result["frames"]),(36,[4,4]))
        self.assertEqual(len(accepted),44)

    def test_warmup_drain_does_not_release_unreturned_objects(self):
        def unexpected(pointer): raise AssertionError("no object was returned")
        lib=SimpleNamespace(amc_encoded_pull=lambda *args:h.a.AM_ERR_SDK_TRY_AGAIN,
            amc_frame_pull=lambda *args:h.a.AM_ERR_SDK_DEADLINE_EXCEEDED,
            amc_encoded_packet_release=unexpected,amc_frame_release=unexpected)
        self.assertEqual(h.drain_warmup(lib,object(),[object()])["frames"],[0])


if __name__=="__main__": unittest.main()
