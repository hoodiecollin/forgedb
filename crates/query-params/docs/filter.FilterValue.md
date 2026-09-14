The typed form of a filter parameter's text, as decided by [`Filter::from_params`].

Classification looks only at the text: number first, then bool, then string. The derived `Deserialize` impl is `untagged`, so a JSON number, bool or string deserializes into the matching variant.
