package io.github.marvinbaudach.reprise.library

import androidx.media3.common.DataReader
import androidx.media3.extractor.DefaultExtractorInput
import io.github.marvinbaudach.reprise.TestFlacFile
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class FlacTailTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val file by lazy {
        TestFlacFile.writeTenMinuteCompressibleUnderstated(folder.newFile("understated.flac")).readBytes()
    }

    @Test
    fun mtp_68_the_last_frame_header_gives_the_samples_a_file_really_holds() {
        val reader = FlacTailReader { position, length -> file.copyOfRange(position.toInt(), position.toInt() + length) }

        val samples = flacSamplesFromLastFrame(file.copyOf(STREAM_INFO_FIELDS_END), file.size.toLong(), reader)

        assertEquals(TestFlacFile.TRUE_SAMPLES, samples)
    }

    @Test
    fun mtp_68_a_tail_that_cannot_be_read_gives_no_sample_count() {
        val samples = flacSamplesFromLastFrame(
            file.copyOf(STREAM_INFO_FIELDS_END),
            file.size.toLong(),
            FlacTailReader { _, _ -> null },
        )

        assertNull(samples)
    }

    @Test
    fun mtp_68_a_tail_without_a_frame_header_gives_no_sample_count() {
        val samples = flacSamplesFromLastFrame(
            file.copyOf(STREAM_INFO_FIELDS_END),
            file.size.toLong(),
            FlacTailReader { _, length -> ByteArray(length) },
        )

        assertNull(samples)
    }

    @Test
    fun mtp_68_the_masked_total_is_what_the_last_frame_says_not_a_guess_from_the_file_length() {
        val reader = FlacTailReader { position, length -> file.copyOfRange(position.toInt(), position.toInt() + length) }
        var offset = 0
        val source = DataReader { target, targetOffset, length ->
            if (offset == file.size) {
                -1
            } else {
                val read = minOf(length, file.size - offset)
                file.copyInto(target, targetOffset, offset, offset + read)
                offset += read
                read
            }
        }
        val input = FlacStreamInfoMaskingExtractorInput(
            DefaultExtractorInput(source, 0, file.size.toLong()),
            reader,
        )

        val head = ByteArray(STREAM_INFO_FIELDS_END).also { input.readFully(it, 0, it.size) }

        val presented = (0..4).fold(0L) { total, index ->
            val byte = head[21 + index].toLong() and if (index == 0) 0x0F else 0xFF
            (total shl 8) or byte
        }
        assertEquals(TestFlacFile.TRUE_SAMPLES, presented)
    }
}
