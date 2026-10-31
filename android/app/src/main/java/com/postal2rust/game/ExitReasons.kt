package com.postal2rust.game

import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.Context
import android.os.Build
import java.io.File
import java.util.Date

/**
 * Records how earlier runs of the game ended (Android 11 and newer), into `<data>/logs/exit.txt`.
 *
 * This is the only record of a native crash that the app itself can read: when the process dies
 * inside native code (for example in the GPU driver), no Rust panic is written, and the system
 * keeps the reason and the native stack (a "tombstone") instead. The system keeps the history
 * across app updates, so a crash of an older build can still be found.
 */
object ExitReasons {
    private const val MAX_TRACE_CHARS = 200_000

    /** Adds the exit reasons that are not in `exit.txt` yet. Returns how many were added. */
    fun record(context: Context, logsDir: File): Int {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return 0
        val am = context.getSystemService(ActivityManager::class.java) ?: return 0
        val seenFile = File(logsDir, "exit-seen.txt")
        val seen = seenFile.takeIf { it.isFile }?.readText()?.trim()?.toLongOrNull() ?: 0L
        val infos = am.getHistoricalProcessExitReasons(context.packageName, 0, 16)
            .filter { it.timestamp > seen }
            .sortedBy { it.timestamp }
        if (infos.isEmpty()) return 0

        logsDir.mkdirs()
        val names = reasonNames()
        File(logsDir, "exit.txt").appendText(buildString {
            for (info in infos) {
                append("--- ${Date(info.timestamp)} (pid ${info.pid})\n")
                append("reason: ${names[info.reason] ?: info.reason}, signal/status: ${info.status}\n")
                append("importance: ${info.importance}\n")
                append("description: ${info.description ?: "-"}\n")
                val trace = try {
                    info.traceInputStream?.bufferedReader()?.use { it.readText() }
                } catch (e: Exception) {
                    null
                }
                if (!trace.isNullOrEmpty()) {
                    append("trace:\n")
                    append(if (trace.length > MAX_TRACE_CHARS) trace.take(MAX_TRACE_CHARS) + "\n[trace cut]\n" else trace)
                    append("\n")
                }
                append("\n")
            }
        })
        seenFile.writeText(infos.last().timestamp.toString())
        return infos.size
    }

    /** Names of the `ApplicationExitInfo.REASON_*` constants, by value (read by reflection, so
     *  that newer reasons show their names on newer phones and nothing needs a newer API level). */
    private fun reasonNames(): Map<Int, String> = try {
        ApplicationExitInfo::class.java.fields
            .filter { it.name.startsWith("REASON_") && it.type == Int::class.javaPrimitiveType }
            .associate { it.getInt(null) to it.name }
    } catch (e: Exception) {
        emptyMap()
    }
}
