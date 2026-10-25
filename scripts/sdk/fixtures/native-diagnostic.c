/* 负例专用：无CRT、无DllMain，仅有与FFmpeg无关的无害导出。 */
__declspec(dllexport) unsigned int sdk_diagnostic_fixture(void) {
    return 1;
}
