package de.reprise.spike

import java.io.FileNotFoundException
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SafAbsenceTest {
    @Test
    fun measuredWrappedFileNotFoundShapeConfirmsAbsence() {
        val missing = FileNotFoundException("Missing file for primary:Music/Reprise/song.flac")
        val error = RuntimeException(
            "Failed to determine if child is child of tree: java.io.FileNotFoundException",
            missing,
        )

        assertTrue(error.confirmsAbsence())
    }

    @Test
    fun measuredMessageWithoutFileNotFoundCauseDoesNotConfirmAbsence() {
        val error = RuntimeException(
            "Failed to determine if child is child of tree: java.io.FileNotFoundException",
        )

        assertFalse(error.confirmsAbsence())
    }

    @Test
    fun bareFileNotFoundConfirmsAbsence() {
        assertTrue(FileNotFoundException("gone").confirmsAbsence())
    }

    @Test
    fun ordinaryRuntimeFailuresDoNotConfirmAbsence() {
        assertFalse(RuntimeException("provider failed").confirmsAbsence())
        assertFalse(IllegalStateException("provider unavailable").confirmsAbsence())
    }

    @Test
    fun securityAnywhereInTheCauseChainDoesNotConfirmAbsence() {
        val error = SecurityException("grant revoked", FileNotFoundException("gone"))

        assertFalse(error.confirmsAbsence())
    }

    @Test
    fun runtimeWrappingSecurityWrappingFileNotFoundDoesNotConfirmAbsence() {
        val error = RuntimeException(
            "query failed",
            SecurityException("grant revoked", FileNotFoundException("gone")),
        )

        assertFalse(error.confirmsAbsence())
    }

    @Test
    fun selfReferencingCauseChainTerminates() {
        val error = object : RuntimeException("loop") {
            override val cause: Throwable
                get() = this
        }

        assertFalse(error.confirmsAbsence())
    }
}
