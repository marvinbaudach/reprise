package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class TitleSearchFieldTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun openingTheSearchFieldFocusesItWithoutAnotherTap() {
        composeSearch()

        compose.onNodeWithText("Search titles").assertIsFocused()
    }

    @Test
    fun backClosesTheOpenSearchField() {
        var visible by mutableStateOf(true)
        compose.setContent {
            MaterialTheme {
                if (visible) {
                    TitleSearchField(searchText = "", search = {}, close = { visible = false })
                }
            }
        }

        compose.activity.onBackPressedDispatcher.onBackPressed()

        compose.onNodeWithText("Search titles").assertDoesNotExist()
    }

    @Test
    fun trailingActionClearsTextBeforeItClosesAnEmptySearch() {
        var searchText by mutableStateOf("slowdive")
        var closeCount = 0
        compose.setContent {
            MaterialTheme {
                TitleSearchField(
                    searchText = searchText,
                    search = { searchText = it },
                    close = { closeCount += 1 },
                )
            }
        }

        compose.onNodeWithContentDescription("Clear search").performClick()
        compose.runOnIdle {
            assertEquals("", searchText)
            assertEquals(0, closeCount)
        }

        compose.onNodeWithContentDescription("Close search").performClick()
        compose.runOnIdle { assertEquals(1, closeCount) }
    }

    private fun composeSearch(
        searchText: String = "",
        search: (String) -> Unit = {},
        close: () -> Unit = {},
    ) {
        compose.setContent {
            MaterialTheme {
                TitleSearchField(searchText = searchText, search = search, close = close)
            }
        }
    }
}
