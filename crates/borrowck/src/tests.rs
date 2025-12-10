//! Tests for the borrow checker.

#[cfg(test)]
mod tests {
    use crate::lifetime::{LifetimeContext, LifetimeId};
    use crate::loans::{Loan, LoanId, LoanKind, LoanSet, Location};
    use crate::moves::{InitStatus, MoveData};
    use crate::places::{Place, PlaceElem, PlaceId};
    use crate::regions::{RegionId, RegionInference, RegionOrigin};
    use roast_common::Span;

    #[test]
    fn test_lifetime_creation() {
        let mut ctx = LifetimeContext::new();
        let l1 = ctx.fresh(Span::dummy());
        let l2 = ctx.fresh(Span::dummy());
        assert_ne!(l1, l2);
        assert!(!l1.is_static());
        assert!(LifetimeId::STATIC.is_static());
    }

    #[test]
    fn test_lifetime_outlives() {
        let mut ctx = LifetimeContext::new();
        let l1 = ctx.fresh(Span::dummy());
        let l2 = ctx.fresh(Span::dummy());
        
        // Static outlives everything
        assert!(ctx.outlives(LifetimeId::STATIC, l1));
        assert!(ctx.outlives(LifetimeId::STATIC, l2));
        
        // Same lifetime outlives itself
        assert!(ctx.outlives(l1, l1));
        
        // Add constraint: l1 outlives l2
        ctx.add_outlives(l1, l2);
        assert!(ctx.outlives(l1, l2));
    }

    #[test]
    fn test_place_creation() {
        let local = PlaceId(0);
        let place = Place::local(local);
        assert!(place.is_local());
        assert!(!place.has_deref());
        assert_eq!(place.depth(), 0);
    }

    #[test]
    fn test_place_projections() {
        let local = PlaceId(0);
        let place = Place::local(local)
            .field(0, None)
            .field(1, None);
        
        assert!(!place.is_local());
        assert_eq!(place.depth(), 2);
        assert!(!place.has_deref());
    }

    #[test]
    fn test_place_deref() {
        let local = PlaceId(0);
        let place = Place::local(local).deref();
        
        assert!(place.has_deref());
        assert_eq!(place.depth(), 1);
    }

    #[test]
    fn test_place_prefix() {
        let local = PlaceId(0);
        let x = Place::local(local);
        let x_f = Place::local(local).field(0, None);
        let x_f_g = Place::local(local).field(0, None).field(1, None);
        
        assert!(x.is_prefix_of(&x_f));
        assert!(x.is_prefix_of(&x_f_g));
        assert!(x_f.is_prefix_of(&x_f_g));
        
        assert!(!x_f.is_prefix_of(&x));
        assert!(!x_f_g.is_prefix_of(&x));
    }

    #[test]
    fn test_place_overlap() {
        let local = PlaceId(0);
        let x = Place::local(local);
        let x_f = Place::local(local).field(0, None);
        let y = Place::local(PlaceId(1));
        
        assert!(x.may_overlap(&x_f));
        assert!(x_f.may_overlap(&x));
        assert!(!x.may_overlap(&y));
    }

    #[test]
    fn test_loan_creation() {
        let place = Place::local(PlaceId(0));
        let loan = Loan::new(
            LoanId(0),
            LoanKind::Shared,
            place.clone(),
            LifetimeId::STATIC,
            Span::dummy(),
            Location::start(),
        );
        
        assert!(!loan.kind.is_mutable());
        assert!(loan.kind.is_shared());
    }

    #[test]
    fn test_loan_conflicts() {
        let place = Place::local(PlaceId(0));
        let shared = Loan::new(
            LoanId(0),
            LoanKind::Shared,
            place.clone(),
            LifetimeId::STATIC,
            Span::dummy(),
            Location::start(),
        );
        
        // Shared borrows don't conflict with each other
        assert!(!shared.conflicts_with(LoanKind::Shared, &place));
        
        // Shared conflicts with mutable
        assert!(shared.conflicts_with(LoanKind::Mutable, &place));
        
        // Mutable conflicts with shared
        let mutable = Loan::new(
            LoanId(1),
            LoanKind::Mutable,
            place.clone(),
            LifetimeId::STATIC,
            Span::dummy(),
            Location::start(),
        );
        assert!(mutable.conflicts_with(LoanKind::Shared, &place));
        assert!(mutable.conflicts_with(LoanKind::Mutable, &place));
    }

    #[test]
    fn test_loan_set() {
        let mut loans = LoanSet::new();
        let place = Place::local(PlaceId(0));
        
        let loan = Loan::new(
            LoanId(0),
            LoanKind::Shared,
            place.clone(),
            LifetimeId::STATIC,
            Span::dummy(),
            Location::start(),
        );
        
        loans.add(loan);
        assert_eq!(loans.len(), 1);
        
        // Check conflicts
        let conflicts = loans.conflicts_with(LoanKind::Mutable, &place);
        assert_eq!(conflicts.len(), 1);
        
        // Remove by ID
        loans.remove(LoanId(0));
        assert!(loans.is_empty());
    }

    #[test]
    fn test_move_data() {
        let mut moves = MoveData::new();
        let place = Place::local(PlaceId(0));
        
        // Initially, place is untracked (assumed initialized)
        assert_eq!(moves.is_initialized(&place), InitStatus::Initialized);
        
        // Record initialization
        moves.record_init(&place, Location::start());
        assert_eq!(moves.is_initialized(&place), InitStatus::Initialized);
        
        // Record move
        moves.record_move(&place, Location::new(0, 1), Span::dummy(), false);
        assert_eq!(moves.is_initialized(&place), InitStatus::Uninitialized);
    }

    #[test]
    fn test_copy_move() {
        let mut moves = MoveData::new();
        let place = Place::local(PlaceId(0));
        
        moves.record_init(&place, Location::start());
        
        // Copy doesn't uninitialize
        moves.record_move(&place, Location::new(0, 1), Span::dummy(), true);
        assert_eq!(moves.is_initialized(&place), InitStatus::Initialized);
    }

    #[test]
    fn test_region_creation() {
        let mut regions = RegionInference::new();
        let r1 = regions.new_anonymous(Location::start());
        let r2 = regions.new_inferred();
        
        assert_ne!(r1, r2);
        assert!(!r1.is_static());
        assert!(RegionId::STATIC.is_static());
    }

    #[test]
    fn test_region_outlives() {
        let mut regions = RegionInference::new();
        let r1 = regions.new_inferred();
        let r2 = regions.new_inferred();
        
        // Static outlives everything
        assert!(regions.outlives(RegionId::STATIC, r1));
        
        // Same region outlives itself
        assert!(regions.outlives(r1, r1));
        
        // Add constraint and solve
        regions.add_outlives(
            r1,
            r2,
            crate::regions::ConstraintOrigin {
                kind: crate::regions::ConstraintKind::Borrow,
                span: Span::dummy(),
            },
        );
        regions.add_live_at(r2, Location::start());
        
        assert!(regions.solve().is_ok());
    }

    #[test]
    fn test_location() {
        let loc = Location::new(0, 0);
        assert_eq!(loc.block, 0);
        assert_eq!(loc.statement, 0);
        
        let next = loc.next();
        assert_eq!(next.statement, 1);
    }
}

