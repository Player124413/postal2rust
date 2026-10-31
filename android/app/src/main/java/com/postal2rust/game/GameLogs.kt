package com.postal2rust.game

import android.content.Context
import android.os.Build
import java.io.File
import java.io.RandomAccessFile
import java.util.Date

/**
 * Collects the game's log files (written by the native side into `<data>/logs/`) into one text
 * file that can be sent or copied. Each file is cut to its last [LIMIT_BYTES] bytes, so the
 * bundle stays small even after a long session.
 */
object GameLogs {
    /** Order matters: the newest crash goes last, where a copied tail of the text ends up. */
    private val FILES = listOf("exit", "game-previous", "logcat-previous", "crash-previous", "game", "logcat", "crash")

    private const val LIMIT_BYTES = 2L shl 20

    /** Writes the bundle to the app cache (`cache/logs/postal2rust-log.txt`) and returns it. */
    fun bundle(context: Context, logsDir: File): File {
        val out = File(context.cacheDir, "logs/postal2rust-log.txt")
        out.parentFile?.mkdirs()
        out.outputStream().buffered().use { stream ->
            stream.write(header(context).toByteArray())
            for (name in FILES) {
                val f = File(logsDir, "$name.txt")
                if (!f.isFile) continue
                stream.write("\n===== $name.txt (${f.length()} bytes) =====\n".toByteArray())
                copyTail(f, stream)
            }
        }
        return out
    }

    /** The last part of the bundle as text, for the clipboard. */
    fun tailText(context: Context, logsDir: File, maxChars: Int): String {
        val text = bundle(context, logsDir).readText()
        return if (text.length > maxChars) text.takeLast(maxChars) else text
    }

    private fun header(context: Context): String {
        val version = try {
            context.packageManager.getPackageInfo(context.packageName, 0).versionName
        } catch (e: Exception) {
            "?"
        }
        return buildString {
            appendLine("POSTAL 2 Rust log")
            appendLine("app: ${context.packageName} $version")
            appendLine("device: ${Build.MANUFACTURER} ${Build.MODEL}, Android ${Build.VERSION.RELEASE} (SDK ${Build.VERSION.SDK_INT})")
            appendLine("abi: ${Build.SUPPORTED_ABIS.joinToString()}")
            appendLine("created: ${Date()}")
        }
    }

    private fun copyTail(f: File, stream: java.io.OutputStream) {
        val length = f.length()
        val start = (length - LIMIT_BYTES).coerceAtLeast(0L)
        RandomAccessFile(f, "r").use { raf ->
            raf.seek(start)
            val bytes = ByteArray((length - start).toInt())
            raf.readFully(bytes)
            var from = 0
            if (start > 0) {
                // Cut in the middle of a line: start at the next line.
                val nl = bytes.indexOf('\n'.code.toByte())
                from = if (nl >= 0) nl + 1 else 0
                stream.write("[... earlier output cut ...]\n".toByteArray())
            }
            stream.write(bytes, from, bytes.size - from)
        }
    }
}
