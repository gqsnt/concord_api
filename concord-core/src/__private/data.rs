use std::{fmt, marker::PhantomData};

/// The parent and inputs of one endpoint.
/// The endpoint's type distinguishes calls with the same field types.
pub struct CallData<Parent, Inputs, Endpoint> {
    /// The handle from which the call was constructed.
    pub __parent: Parent,
    /// The call's own inputs, in declaration order.
    pub __inputs: Inputs,
    endpoint: PhantomData<fn(Endpoint) -> Endpoint>,
}

impl<Parent, Inputs, Endpoint> CallData<Parent, Inputs, Endpoint> {
    /// Store the endpoint's parent and its inputs in declaration order.
    pub fn new(parent: Parent, inputs: Inputs) -> Self {
        Self {
            __parent: parent,
            __inputs: inputs,
            endpoint: PhantomData,
        }
    }
}

impl<Parent: Clone, Inputs: Clone, Endpoint> Clone for CallData<Parent, Inputs, Endpoint> {
    fn clone(&self) -> Self {
        Self::new(self.__parent.clone(), self.__inputs.clone())
    }
}

impl<Parent, Inputs, Endpoint> fmt::Debug for CallData<Parent, Inputs, Endpoint> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallData").finish_non_exhaustive()
    }
}

// Invariant in the call and its key so slots cannot cross endpoints.
type SlotIdentity<C, Key> = fn(C, Key) -> (C, Key);

/// A pager slot, identified by the exact call, its input position and a private key.
// PANIC-SAFETY: A slot holds no data to print and callers never name it; no panic lint is relaxed.
#[allow(missing_debug_implementations)]
pub struct InputSlot<C, const N: usize, Key>(PhantomData<SlotIdentity<C, Key>>);

impl<C, const N: usize, Key> InputSlot<C, N, Key> {
    /// The slot passed to an authored pager.
    pub const NEW: Self = Self(PhantomData);
}

impl<C, const N: usize, Key> Copy for InputSlot<C, N, Key> {}

impl<C, const N: usize, Key> Clone for InputSlot<C, N, Key> {
    fn clone(&self) -> Self {
        *self
    }
}

/// The marker for the client-wide types used by generated calls.
/// Each client's own `Call<ClientKey>` is a distinct type.
#[derive(Debug)]
pub struct ClientKey;

/// The author-declared problem carried by this client.
pub trait ClientProblem {
    /// The problem type shared by every endpoint of the client.
    type Problem: Send + Sync + 'static;
}

/// A type stored by a client, scope, or call.
pub trait StoredType<const N: usize> {
    /// The owned type at this storage position.
    type Value: Clone + Send + Sync + 'static;
}

/// A generated endpoint type reached through the client key.
pub trait EndpointType<const N: usize> {
    /// The marker or named state of an endpoint.
    type Value;
}

macro_rules! endpoint_input_groups {
    (($($input:ident),*); $group:ident: $next:ident $(, $rest:ident: $tail:ident)*) => {
        /// Types authored together in an endpoint signature. Call storage checks their bounds.
        pub trait $group<const N: usize, const GROUP: usize = 0> {
            $(
                /// The authored type at this position in the group.
                type $input;
            )*
            /// The last authored type in the group.
            type $next;
        }
        endpoint_input_groups!(($($input,)* $next); $($rest: $tail),*);
    };
    ($previous:tt;) => {};
}

endpoint_input_groups! {
    (); EndpointInputs1: Input0, EndpointInputs2: Input1, EndpointInputs3: Input2,
    EndpointInputs4: Input3, EndpointInputs5: Input4, EndpointInputs6: Input5,
    EndpointInputs7: Input6, EndpointInputs8: Input7, EndpointInputs9: Input8,
    EndpointInputs10: Input9, EndpointInputs11: Input10, EndpointInputs12: Input11,
    EndpointInputs13: Input12, EndpointInputs14: Input13, EndpointInputs15: Input14,
    EndpointInputs16: Input15
}

/// The borrowed view of a client or scope passed to authored expressions.
pub trait OwnerView<const N: usize> {
    /// The generated view, tied to the lifetime of the handle's stored inputs.
    type View<'a>;
}
