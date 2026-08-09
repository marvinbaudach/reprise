package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class LibraryFolderStatesTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun emptyLibraryNamesTheExpectedFolderBeforeOpeningThePicker() {
        var pickerRequests = 0
        compose.setContent {
            MaterialTheme {
                NoFolderScreen(message = null, chooseFolder = { pickerRequests += 1 })
            }
        }

        compose.onNodeWithText(
            "Choose Music/Reprise to build this device's library. " +
                "If it is not available, choose Music.",
        ).assertIsDisplayed()
        compose.onNodeWithText("Choose folder").performClick()

        assertEquals(1, pickerRequests)
    }
}
